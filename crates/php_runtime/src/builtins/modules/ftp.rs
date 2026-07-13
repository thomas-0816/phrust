//! Loopback-gated FTP control-channel subset.

use super::core::{argument_type_error, arity_error, int_arg, string_arg};
use crate::builtins::{BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult};
use crate::{
    BuiltinError, ClassEntry, ClassFlags, FtpOptionValue, ObjectRef, PhpArray, RuntimeSourceSpan,
    Value, normalize_class_name,
};
use std::convert::TryFrom;
use std::path::{Path, PathBuf};

const FTP_CONNECTION_CLASS: &str = "FTP\\Connection";
const FTP_ID_PROPERTY: &str = "__phrust_ftp_id";
const DEFAULT_FTP_PORT: u16 = 21;
const DEFAULT_FTP_TIMEOUT_SECONDS: u64 = 90;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("ftp_alloc", builtin_ftp_alloc, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_append", builtin_ftp_append, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_cdup", builtin_ftp_cdup, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_chdir", builtin_ftp_chdir, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_chmod", builtin_ftp_chmod, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_close", builtin_ftp_close, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ftp_connect",
        builtin_ftp_connect,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("ftp_delete", builtin_ftp_delete, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_exec", builtin_ftp_exec, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_fget", builtin_ftp_fget, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_fput", builtin_ftp_fput, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_get", builtin_ftp_get, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ftp_get_option",
        builtin_ftp_get_option,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("ftp_login", builtin_ftp_login, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_mdtm", builtin_ftp_mdtm, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_mkdir", builtin_ftp_mkdir, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_mlsd", builtin_ftp_mlsd, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ftp_nb_continue",
        builtin_ftp_nb_continue,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ftp_nb_fget",
        builtin_ftp_nb_fget,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ftp_nb_fput",
        builtin_ftp_nb_fput,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("ftp_nb_get", builtin_ftp_nb_get, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_nb_put", builtin_ftp_nb_put, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_nlist", builtin_ftp_nlist, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_pasv", builtin_ftp_pasv, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_put", builtin_ftp_put, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_pwd", builtin_ftp_pwd, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_quit", builtin_ftp_close, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_raw", builtin_ftp_raw, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ftp_rawlist",
        builtin_ftp_rawlist,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("ftp_rename", builtin_ftp_rename, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_rmdir", builtin_ftp_rmdir, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ftp_set_option",
        builtin_ftp_set_option,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("ftp_site", builtin_ftp_site, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftp_size", builtin_ftp_size, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ftp_ssl_connect",
        builtin_ftp_ssl_connect,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ftp_systype",
        builtin_ftp_systype,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_ftp_connect(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("ftp_connect", "one to three arguments"));
    }
    let host = string_arg("ftp_connect", &args[0])?.to_string();
    let port = if let Some(value) = args.get(1) {
        port_arg("ftp_connect", value)?
    } else {
        DEFAULT_FTP_PORT
    };
    let timeout = if let Some(value) = args.get(2) {
        timeout_arg("ftp_connect", value)?
    } else {
        DEFAULT_FTP_TIMEOUT_SECONDS
    };
    if !context.network_requests_enabled() {
        return Ok(Value::Bool(false));
    }
    let allow_configured_live_endpoint = context
        .env_value("PHRUST_FTP_LIVE_ENDPOINT")
        .is_some_and(|endpoint| ftp_endpoint_matches(endpoint, &host, port));
    match context
        .ftp_state()
        .connect(&host, port, timeout, allow_configured_live_endpoint)
    {
        Ok(id) => Ok(Value::Object(ftp_object(id))),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_ssl_connect(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("ftp_ssl_connect", "one to three arguments"));
    }
    let _ = string_arg("ftp_ssl_connect", &args[0])?;
    if let Some(value) = args.get(1) {
        let _ = port_arg("ftp_ssl_connect", value)?;
    }
    if let Some(value) = args.get(2) {
        let _ = timeout_arg("ftp_ssl_connect", value)?;
    }
    Ok(Value::Bool(false))
}

fn builtin_ftp_login(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("ftp_login", "three arguments"));
    }
    let id = ftp_id_arg("ftp_login", &args[0])?;
    let user = string_arg("ftp_login", &args[1])?.to_string();
    let password = string_arg("ftp_login", &args[2])?.to_string();
    match context.ftp_state().login(id, &user, &password) {
        Ok(success) => Ok(Value::Bool(success)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_pwd(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ftp_pwd", "one argument"));
    }
    let id = ftp_id_arg("ftp_pwd", &args[0])?;
    match context.ftp_state().pwd(id) {
        Ok(Some(path)) => Ok(Value::string(path)),
        Ok(None) | Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_chdir(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ftp_chdir", "two arguments"));
    }
    let id = ftp_id_arg("ftp_chdir", &args[0])?;
    let path = string_arg("ftp_chdir", &args[1])?.to_string();
    match context.ftp_state().chdir(id, &path) {
        Ok(success) => Ok(Value::Bool(success)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_cdup(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ftp_cdup", "one argument"));
    }
    let id = ftp_id_arg("ftp_cdup", &args[0])?;
    match context.ftp_state().cdup(id) {
        Ok(success) => Ok(Value::Bool(success)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_exec(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ftp_exec", "two arguments"));
    }
    let id = ftp_id_arg("ftp_exec", &args[0])?;
    let command = string_arg("ftp_exec", &args[1])?.to_string();
    match context.ftp_state().exec(id, &command) {
        Ok(success) => Ok(Value::Bool(success)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_raw(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ftp_raw", "two arguments"));
    }
    let id = ftp_id_arg("ftp_raw", &args[0])?;
    let command = string_arg("ftp_raw", &args[1])?.to_string();
    match context.ftp_state().raw(id, &command) {
        Ok(lines) => Ok(Value::Array(PhpArray::from_packed(
            lines.into_iter().map(Value::string).collect(),
        ))),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_mkdir(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ftp_mkdir", "two arguments"));
    }
    let id = ftp_id_arg("ftp_mkdir", &args[0])?;
    let path = string_arg("ftp_mkdir", &args[1])?.to_string();
    match context.ftp_state().mkdir(id, &path) {
        Ok(Some(path)) => Ok(Value::string(path)),
        Ok(None) | Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_rmdir(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ftp_rmdir", "two arguments"));
    }
    let id = ftp_id_arg("ftp_rmdir", &args[0])?;
    let path = string_arg("ftp_rmdir", &args[1])?.to_string();
    match context.ftp_state().rmdir(id, &path) {
        Ok(success) => Ok(Value::Bool(success)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_delete(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ftp_delete", "two arguments"));
    }
    let id = ftp_id_arg("ftp_delete", &args[0])?;
    let path = string_arg("ftp_delete", &args[1])?.to_string();
    match context.ftp_state().delete(id, &path) {
        Ok(success) => Ok(Value::Bool(success)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_rename(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("ftp_rename", "three arguments"));
    }
    let id = ftp_id_arg("ftp_rename", &args[0])?;
    let from = string_arg("ftp_rename", &args[1])?.to_string();
    let to = string_arg("ftp_rename", &args[2])?.to_string();
    match context.ftp_state().rename(id, &from, &to) {
        Ok(success) => Ok(Value::Bool(success)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_site(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ftp_site", "two arguments"));
    }
    let id = ftp_id_arg("ftp_site", &args[0])?;
    let command = string_arg("ftp_site", &args[1])?.to_string();
    match context.ftp_state().site(id, &command) {
        Ok(success) => Ok(Value::Bool(success)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_alloc(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("ftp_alloc", "two or three arguments"));
    }
    let id = ftp_id_arg("ftp_alloc", &args[0])?;
    let size = int_arg("ftp_alloc", &args[1])?;
    match context.ftp_state().alloc(id, size) {
        Ok((success, response)) => {
            if let Some(Value::Reference(cell)) = args.get(2)
                && let Some(response) = response
            {
                cell.set(Value::string(response));
            }
            Ok(Value::Bool(success))
        }
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_chmod(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("ftp_chmod", "three arguments"));
    }
    let id = ftp_id_arg("ftp_chmod", &args[0])?;
    let permissions = int_arg("ftp_chmod", &args[1])?;
    let path = string_arg("ftp_chmod", &args[2])?.to_string();
    match context.ftp_state().chmod(id, permissions, &path) {
        Ok(Some(permissions)) => Ok(Value::Int(permissions)),
        Ok(None) | Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_systype(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ftp_systype", "one argument"));
    }
    let id = ftp_id_arg("ftp_systype", &args[0])?;
    match context.ftp_state().systype(id) {
        Ok(Some(system)) => Ok(Value::string(system)),
        Ok(None) | Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_size(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ftp_size", "two arguments"));
    }
    let id = ftp_id_arg("ftp_size", &args[0])?;
    let path = string_arg("ftp_size", &args[1])?.to_string();
    match context.ftp_state().size(id, &path) {
        Ok(size) => Ok(Value::Int(size)),
        Err(_) => Ok(Value::Int(-1)),
    }
}

fn builtin_ftp_mdtm(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ftp_mdtm", "two arguments"));
    }
    let id = ftp_id_arg("ftp_mdtm", &args[0])?;
    let path = string_arg("ftp_mdtm", &args[1])?.to_string();
    match context.ftp_state().mdtm(id, &path) {
        Ok(timestamp) => Ok(Value::Int(timestamp)),
        Err(_) => Ok(Value::Int(-1)),
    }
}

fn builtin_ftp_pasv(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ftp_pasv", "two arguments"));
    }
    let id = ftp_id_arg("ftp_pasv", &args[0])?;
    let enabled = bool_arg(&args[1]);
    match context.ftp_state().set_passive(id, enabled) {
        Ok(success) => Ok(Value::Bool(success)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_nlist(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ftp_nlist", "two arguments"));
    }
    let id = ftp_id_arg("ftp_nlist", &args[0])?;
    let path = string_arg("ftp_nlist", &args[1])?.to_string();
    match context.ftp_state().nlist(id, &path) {
        Ok(Some(lines)) => Ok(string_array(lines)),
        Ok(None) | Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_rawlist(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("ftp_rawlist", "two or three arguments"));
    }
    let id = ftp_id_arg("ftp_rawlist", &args[0])?;
    let path = string_arg("ftp_rawlist", &args[1])?.to_string();
    let recursive = args.get(2).is_some_and(bool_arg);
    match context.ftp_state().rawlist(id, &path, recursive) {
        Ok(Some(lines)) => Ok(string_array(lines)),
        Ok(None) | Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_mlsd(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ftp_mlsd", "two arguments"));
    }
    let id = ftp_id_arg("ftp_mlsd", &args[0])?;
    let path = string_arg("ftp_mlsd", &args[1])?.to_string();
    match context.ftp_state().mlsd(id, &path) {
        Ok(Some(lines)) => Ok(string_array(lines)),
        Ok(None) | Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_get(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    ftp_get_impl(context, args).map(Value::Bool)
}

fn builtin_ftp_nb_get(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    Ok(Value::Int(if ftp_get_impl(context, args)? { 1 } else { 0 }))
}

fn ftp_get_impl(context: &mut BuiltinContext<'_>, args: Vec<Value>) -> Result<bool, BuiltinError> {
    if !(3..=5).contains(&args.len()) {
        return Err(arity_error("ftp_get", "three to five arguments"));
    }
    let id = ftp_id_arg("ftp_get", &args[0])?;
    let local = string_arg("ftp_get", &args[1])?.to_string();
    let remote = string_arg("ftp_get", &args[2])?.to_string();
    let mode = mode_arg("ftp_get", args.get(3))?;
    let offset = offset_arg("ftp_get", args.get(4))?;
    let local_path = resolve_local_path(context.cwd(), &local);
    match context.ftp_state().retrieve(id, &remote, mode, offset) {
        Ok(Some(bytes)) => Ok(std::fs::write(local_path, bytes).is_ok()),
        Ok(None) | Err(_) => Ok(false),
    }
}

fn builtin_ftp_put(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    ftp_put_impl(context, args, false, false).map(Value::Bool)
}

fn builtin_ftp_append(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    ftp_put_impl(context, args, true, false).map(Value::Bool)
}

fn builtin_ftp_nb_put(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    Ok(Value::Int(if ftp_put_impl(context, args, false, true)? {
        1
    } else {
        0
    }))
}

fn ftp_put_impl(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    append: bool,
    nb: bool,
) -> Result<bool, BuiltinError> {
    let max_args = if append { 4 } else { 5 };
    if !(3..=max_args).contains(&args.len()) {
        return Err(arity_error(
            if append {
                "ftp_append"
            } else if nb {
                "ftp_nb_put"
            } else {
                "ftp_put"
            },
            if append {
                "three or four arguments"
            } else {
                "three to five arguments"
            },
        ));
    }
    let name = if append {
        "ftp_append"
    } else if nb {
        "ftp_nb_put"
    } else {
        "ftp_put"
    };
    let id = ftp_id_arg(name, &args[0])?;
    let remote = string_arg(name, &args[1])?.to_string();
    let local = string_arg(name, &args[2])?.to_string();
    let mode = mode_arg(name, args.get(3))?;
    let offset = if append {
        0
    } else {
        offset_arg(name, args.get(4))?
    };
    let local_path = resolve_local_path(context.cwd(), &local);
    let Ok(bytes) = std::fs::read(local_path) else {
        return Ok(false);
    };
    match context
        .ftp_state()
        .store(id, &remote, &bytes, mode, offset, append)
    {
        Ok(success) => Ok(success),
        Err(_) => Ok(false),
    }
}

fn builtin_ftp_fget(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    validate_stream_transfer_args("ftp_fget", args, false)?;
    Ok(Value::Bool(false))
}

fn builtin_ftp_fput(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    validate_stream_transfer_args("ftp_fput", args, true)?;
    Ok(Value::Bool(false))
}

fn builtin_ftp_nb_fget(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    validate_stream_transfer_args("ftp_nb_fget", args, false)?;
    Ok(Value::Int(0))
}

fn builtin_ftp_nb_fput(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    validate_stream_transfer_args("ftp_nb_fput", args, true)?;
    Ok(Value::Int(0))
}

fn builtin_ftp_nb_continue(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ftp_nb_continue", "one argument"));
    }
    let _ = ftp_id_arg("ftp_nb_continue", &args[0])?;
    Ok(Value::Int(1))
}

fn builtin_ftp_get_option(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ftp_get_option", "two arguments"));
    }
    let id = ftp_id_arg("ftp_get_option", &args[0])?;
    let option = int_arg("ftp_get_option", &args[1])?;
    match context.ftp_state().get_option(id, option) {
        Ok(Some(FtpOptionValue::Int(value))) => Ok(Value::Int(value)),
        Ok(Some(FtpOptionValue::Bool(value))) => Ok(Value::Bool(value)),
        Ok(None) | Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_set_option(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("ftp_set_option", "three arguments"));
    }
    let id = ftp_id_arg("ftp_set_option", &args[0])?;
    let option = int_arg("ftp_set_option", &args[1])?;
    let value = match option {
        0 => FtpOptionValue::Int(int_arg("ftp_set_option", &args[2])?),
        1 | 2 => FtpOptionValue::Bool(bool_arg(&args[2])),
        _ => return Ok(Value::Bool(false)),
    };
    match context.ftp_state().set_option(id, option, value) {
        Ok(true) => Ok(Value::Bool(true)),
        Ok(false) | Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_ftp_close(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ftp_close", "one argument"));
    }
    let id = ftp_id_arg("ftp_close", &args[0])?;
    match context.ftp_state().close(id) {
        Ok(success) => Ok(Value::Bool(success)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn ftp_id_arg(name: &str, value: &Value) -> Result<i64, BuiltinError> {
    let Value::Object(object) = value else {
        return Err(argument_type_error(
            name,
            "#1 ($ftp)",
            FTP_CONNECTION_CLASS,
            value,
        ));
    };
    if normalize_class_name(&object.class_name()) != "ftp\\connection" {
        return Err(argument_type_error(
            name,
            "#1 ($ftp)",
            FTP_CONNECTION_CLASS,
            value,
        ));
    }
    match object.get_property(FTP_ID_PROPERTY) {
        Some(Value::Int(id)) if id > 0 => Ok(id),
        _ => Err(BuiltinError::new(
            "E_PHP_RUNTIME_FTP_INVALID",
            format!("{name}(): FTP\\Connection object is no longer valid"),
        )),
    }
}

fn port_arg(name: &str, value: &Value) -> Result<u16, BuiltinError> {
    let port = int_arg(name, value)?;
    u16::try_from(port).map_err(|_| {
        BuiltinError::new(
            "E_PHP_RUNTIME_VALUE",
            format!("{name}(): port is outside the supported range"),
        )
    })
}

fn timeout_arg(name: &str, value: &Value) -> Result<u64, BuiltinError> {
    let timeout = int_arg(name, value)?;
    if timeout <= 0 {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_VALUE",
            format!("{name}(): timeout must be greater than zero"),
        ));
    }
    u64::try_from(timeout).map_err(|_| {
        BuiltinError::new(
            "E_PHP_RUNTIME_VALUE",
            format!("{name}(): timeout is outside the supported range"),
        )
    })
}

fn ftp_endpoint_matches(endpoint: &str, host: &str, port: u16) -> bool {
    let (endpoint_host, endpoint_port) = match endpoint.rsplit_once(':') {
        Some((endpoint_host, endpoint_port)) => {
            let Ok(endpoint_port) = endpoint_port.parse::<u16>() else {
                return false;
            };
            (endpoint_host, endpoint_port)
        }
        None => (endpoint, DEFAULT_FTP_PORT),
    };
    endpoint_host == host && endpoint_port == port
}

fn mode_arg(name: &str, value: Option<&Value>) -> Result<i64, BuiltinError> {
    let mode = match value {
        Some(value) => int_arg(name, value)?,
        None => 2,
    };
    if mode == 1 || mode == 2 {
        Ok(mode)
    } else {
        Err(BuiltinError::new(
            "E_PHP_RUNTIME_VALUE",
            format!("{name}(): mode must be either FTP_ASCII or FTP_BINARY"),
        ))
    }
}

fn offset_arg(name: &str, value: Option<&Value>) -> Result<i64, BuiltinError> {
    let offset = match value {
        Some(value) => int_arg(name, value)?,
        None => 0,
    };
    if offset >= 0 {
        Ok(offset)
    } else {
        Err(BuiltinError::new(
            "E_PHP_RUNTIME_VALUE",
            format!("{name}(): offset must be non-negative"),
        ))
    }
}

fn validate_stream_transfer_args(
    name: &str,
    args: Vec<Value>,
    remote_first: bool,
) -> Result<(), BuiltinError> {
    if !(4..=5).contains(&args.len()) {
        return Err(arity_error(name, "four or five arguments"));
    }
    let _ = ftp_id_arg(name, &args[0])?;
    if remote_first {
        let _ = string_arg(name, &args[1])?;
    } else {
        let _ = string_arg(name, &args[2])?;
    }
    let _ = mode_arg(name, args.get(3))?;
    let _ = offset_arg(name, args.get(4))?;
    Ok(())
}

fn resolve_local_path(cwd: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

fn bool_arg(value: &Value) -> bool {
    match value {
        Value::Bool(value) => *value,
        Value::Null | Value::Uninitialized => false,
        Value::Int(value) => *value != 0,
        Value::Float(value) => value.to_f64() != 0.0,
        Value::String(value) => {
            let bytes = value.as_bytes();
            !bytes.is_empty() && bytes != b"0"
        }
        Value::Array(array) => !array.is_empty(),
        Value::Object(_) | Value::Resource(_) | Value::Fiber(_) | Value::Generator(_) => true,
        Value::Callable(_) => true,
        Value::Reference(cell) => bool_arg(&cell.get()),
    }
}

fn string_array(lines: Vec<String>) -> Value {
    Value::Array(PhpArray::from_packed(
        lines.into_iter().map(Value::string).collect(),
    ))
}

fn ftp_object(id: i64) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&ftp_runtime_class(), FTP_CONNECTION_CLASS);
    object.set_property(FTP_ID_PROPERTY, Value::Int(id));
    object
}

fn ftp_runtime_class() -> ClassEntry {
    ClassEntry {
        name: "ftp\\connection".to_owned().into(),
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
    use crate::ArrayKey;
    use crate::OutputBuffer;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn call_with_context(context: &mut BuiltinContext<'_>, name: &str, args: Vec<Value>) -> Value {
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .expect("entry")
            .function()(context, args, RuntimeSourceSpan::default())
        .expect("builtin succeeds")
    }

    #[test]
    fn plain_loopback_control_channel_uses_suppaftp_backend() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind fake ftp");
        let port = listener.local_addr().expect("local addr").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept ftp control");
            stream
                .write_all(b"220 phrust ftp ready\r\n")
                .expect("greeting");
            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut data_listener = None;
            loop {
                let mut line = String::new();
                let read = reader.read_line(&mut line).expect("read command");
                if read == 0 {
                    break;
                }
                match line.trim_end_matches(['\r', '\n']) {
                    "USER user" => stream.write_all(b"331 password please\r\n").expect("user"),
                    "PASS pass" => stream.write_all(b"230 logged in\r\n").expect("pass"),
                    "PWD" => stream.write_all(b"257 \"/pub\" is cwd\r\n").expect("pwd"),
                    "CWD /tmp" => stream.write_all(b"250 changed\r\n").expect("cwd"),
                    "CDUP" => stream.write_all(b"250 parent\r\n").expect("cdup"),
                    "MKD new" => stream.write_all(b"257 \"new\" created\r\n").expect("mkd"),
                    "RMD old" => stream.write_all(b"250 removed\r\n").expect("rmd"),
                    "DELE stale.txt" => stream.write_all(b"250 deleted\r\n").expect("dele"),
                    "RNFR from.txt" => stream.write_all(b"350 ready\r\n").expect("rnfr"),
                    "RNTO to.txt" => stream.write_all(b"250 renamed\r\n").expect("rnto"),
                    "SITE HELP" => stream.write_all(b"200 site ok\r\n").expect("site"),
                    "SITE EXEC uptime" => stream.write_all(b"200 exec ok\r\n").expect("exec"),
                    "SITE CHMOD 644 file.txt" => {
                        stream.write_all(b"200 chmod ok\r\n").expect("chmod")
                    }
                    "ALLO 128" => stream.write_all(b"202 allo ignored\r\n").expect("allo"),
                    "SIZE file.txt" => stream.write_all(b"213 42\r\n").expect("size"),
                    "MDTM file.txt" => stream.write_all(b"213 20250102030405\r\n").expect("mdtm"),
                    "SYST" => stream.write_all(b"215 UNIX Type: L8\r\n").expect("syst"),
                    "TYPE I" => stream.write_all(b"200 type set\r\n").expect("type"),
                    "PASV" => {
                        let listener =
                            TcpListener::bind(("127.0.0.1", 0)).expect("bind data listener");
                        let data_port = listener.local_addr().expect("data addr").port();
                        let p1 = data_port / 256;
                        let p2 = data_port % 256;
                        data_listener = Some(listener);
                        write!(
                            stream,
                            "227 Entering Passive Mode (127,0,0,1,{p1},{p2})\r\n"
                        )
                        .expect("pasv");
                    }
                    "NLST /pub" => {
                        stream
                            .write_all(b"150 opening data\r\n")
                            .expect("nlst open");
                        let (mut data, _) = data_listener
                            .take()
                            .expect("data listener")
                            .accept()
                            .expect("accept data");
                        data.write_all(b"one.txt\r\ntwo.txt\r\n")
                            .expect("nlst data");
                        drop(data);
                        stream
                            .write_all(b"226 transfer complete\r\n")
                            .expect("nlst done");
                    }
                    "LIST /pub" => {
                        stream
                            .write_all(b"150 opening data\r\n")
                            .expect("list open");
                        let (mut data, _) = data_listener
                            .take()
                            .expect("data listener")
                            .accept()
                            .expect("accept data");
                        data.write_all(b"-rw-r--r-- 1 owner group 3 Jan 01 00:00 one.txt\r\n")
                            .expect("list data");
                        drop(data);
                        stream
                            .write_all(b"226 transfer complete\r\n")
                            .expect("list done");
                    }
                    "RETR remote.txt" => {
                        stream
                            .write_all(b"150 opening data\r\n")
                            .expect("retr open");
                        let (mut data, _) = data_listener
                            .take()
                            .expect("data listener")
                            .accept()
                            .expect("accept data");
                        data.write_all(b"downloaded").expect("retr data");
                        drop(data);
                        stream
                            .write_all(b"226 transfer complete\r\n")
                            .expect("retr done");
                    }
                    "STOR uploaded.txt" => {
                        stream
                            .write_all(b"150 opening data\r\n")
                            .expect("stor open");
                        let (mut data, _) = data_listener
                            .take()
                            .expect("data listener")
                            .accept()
                            .expect("accept data");
                        let mut bytes = Vec::new();
                        data.read_to_end(&mut bytes).expect("stor data");
                        assert_eq!(bytes, b"uploaded");
                        stream
                            .write_all(b"226 transfer complete\r\n")
                            .expect("stor done");
                    }
                    "APPE appended.txt" => {
                        stream
                            .write_all(b"150 opening data\r\n")
                            .expect("appe open");
                        let (mut data, _) = data_listener
                            .take()
                            .expect("data listener")
                            .accept()
                            .expect("accept data");
                        let mut bytes = Vec::new();
                        data.read_to_end(&mut bytes).expect("appe data");
                        assert_eq!(bytes, b"uploaded");
                        stream
                            .write_all(b"226 transfer complete\r\n")
                            .expect("appe done");
                    }
                    "QUIT" => {
                        stream.write_all(b"221 bye\r\n").expect("quit");
                        break;
                    }
                    _ => stream.write_all(b"500 unknown\r\n").expect("unknown"),
                }
            }
        });

        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        context.set_network_requests_enabled(true);
        let ftp = call_with_context(
            &mut context,
            "ftp_connect",
            vec![
                Value::string("127.0.0.1"),
                Value::Int(i64::from(port)),
                Value::Int(1),
            ],
        );
        assert!(matches!(ftp, Value::Object(_)), "{ftp:?}");
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_login",
                vec![ftp.clone(), Value::string("user"), Value::string("pass")]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_context(&mut context, "ftp_pwd", vec![ftp.clone()]),
            Value::string("/pub")
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_chdir",
                vec![ftp.clone(), Value::string("/tmp")]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_context(&mut context, "ftp_cdup", vec![ftp.clone()]),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_mkdir",
                vec![ftp.clone(), Value::string("new")]
            ),
            Value::string("new")
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_rmdir",
                vec![ftp.clone(), Value::string("old")]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_delete",
                vec![ftp.clone(), Value::string("stale.txt")]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_rename",
                vec![
                    ftp.clone(),
                    Value::string("from.txt"),
                    Value::string("to.txt")
                ]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_site",
                vec![ftp.clone(), Value::string("HELP")]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_exec",
                vec![ftp.clone(), Value::string("uptime")]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_chmod",
                vec![ftp.clone(), Value::Int(0o644), Value::string("file.txt")]
            ),
            Value::Int(0o644)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_alloc",
                vec![ftp.clone(), Value::Int(128)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_size",
                vec![ftp.clone(), Value::string("file.txt")]
            ),
            Value::Int(42)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_mdtm",
                vec![ftp.clone(), Value::string("file.txt")]
            ),
            Value::Int(20250102030405)
        );
        assert_eq!(
            call_with_context(&mut context, "ftp_systype", vec![ftp.clone()]),
            Value::string("UNIX Type: L8")
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_get_option",
                vec![ftp.clone(), Value::Int(0)]
            ),
            Value::Int(1)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_set_option",
                vec![ftp.clone(), Value::Int(2), Value::Bool(false)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_get_option",
                vec![ftp.clone(), Value::Int(2)]
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_pasv",
                vec![ftp.clone(), Value::Bool(true)]
            ),
            Value::Bool(true)
        );
        let nlist = call_with_context(
            &mut context,
            "ftp_nlist",
            vec![ftp.clone(), Value::string("/pub")],
        );
        let Value::Array(nlist) = nlist else {
            panic!("nlist response should be array");
        };
        assert_eq!(
            nlist.get(&ArrayKey::Int(0)),
            Some(&Value::string("one.txt"))
        );
        assert_eq!(
            nlist.get(&ArrayKey::Int(1)),
            Some(&Value::string("two.txt"))
        );
        let rawlist = call_with_context(
            &mut context,
            "ftp_rawlist",
            vec![ftp.clone(), Value::string("/pub")],
        );
        let Value::Array(rawlist) = rawlist else {
            panic!("rawlist response should be array");
        };
        assert_eq!(
            rawlist.get(&ArrayKey::Int(0)),
            Some(&Value::string(
                "-rw-r--r-- 1 owner group 3 Jan 01 00:00 one.txt"
            ))
        );
        let temp = std::env::temp_dir().join(format!("phrust-ftp-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp);
        let download_path = temp.join("download.txt");
        let upload_path = temp.join("upload.txt");
        std::fs::write(&upload_path, b"uploaded").expect("write upload fixture");
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_get",
                vec![
                    ftp.clone(),
                    Value::string(download_path.to_string_lossy().into_owned()),
                    Value::string("remote.txt")
                ]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            std::fs::read(&download_path).expect("downloaded file"),
            b"downloaded"
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_put",
                vec![
                    ftp.clone(),
                    Value::string("uploaded.txt"),
                    Value::string(upload_path.to_string_lossy().into_owned())
                ]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_append",
                vec![
                    ftp.clone(),
                    Value::string("appended.txt"),
                    Value::string(upload_path.to_string_lossy().into_owned())
                ]
            ),
            Value::Bool(true)
        );
        let _ = std::fs::remove_file(download_path);
        let _ = std::fs::remove_file(upload_path);
        let _ = std::fs::remove_dir(temp);
        let raw = call_with_context(
            &mut context,
            "ftp_raw",
            vec![ftp.clone(), Value::string("SYST")],
        );
        let Value::Array(lines) = raw else {
            panic!("raw response should be array");
        };
        assert_eq!(
            lines.get(&ArrayKey::Int(0)),
            Some(&Value::string("215 UNIX Type: L8"))
        );
        assert_eq!(
            call_with_context(&mut context, "ftp_close", vec![ftp]),
            Value::Bool(true)
        );
        server.join().expect("fake server exits");
    }

    #[test]
    fn network_gate_keeps_connect_disabled_by_default() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        assert_eq!(
            call_with_context(
                &mut context,
                "ftp_connect",
                vec![Value::string("127.0.0.1"), Value::Int(21), Value::Int(1)]
            ),
            Value::Bool(false)
        );
    }
}
