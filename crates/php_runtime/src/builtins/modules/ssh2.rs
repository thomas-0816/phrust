//! Deterministic SSH2 facade with opt-in libssh2 backend behavior.

use super::core::{argument_type_error, arity_error, int_arg, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, Ssh2FingerprintHash,
};
use crate::{
    ArrayKey, BuiltinError, ClassEntry, ClassFlags, ObjectRef, PhpArray, PhpString,
    RuntimeSourceSpan, StreamFlags, StreamMetadata, Value, normalize_class_name,
};
use std::path::PathBuf;

const SSH2_SESSION_CLASS: &str = "SSH2\\Session";
const SSH2_SFTP_CLASS: &str = "SSH2\\Sftp";
const SSH2_SESSION_ID_PROPERTY: &str = "__phrust_ssh2_session_id";
const SSH2_SFTP_ID_PROPERTY: &str = "__phrust_ssh2_sftp_id";
const SSH2_BACKEND_ERROR: &str = "SSH2 backend is not configured";

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "ssh2_auth_hostbased_file",
        builtin_ssh2_auth_hostbased_file,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_auth_none",
        builtin_ssh2_auth_none,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_auth_password",
        builtin_ssh2_auth_password,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_auth_pubkey_file",
        builtin_ssh2_auth_pubkey_file,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_connect",
        builtin_ssh2_connect,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_disconnect",
        builtin_ssh2_disconnect,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("ssh2_exec", builtin_ssh2_exec, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ssh2_fingerprint",
        builtin_ssh2_fingerprint,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_forward_accept",
        builtin_ssh2_forward_accept,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_forward_listen",
        builtin_ssh2_forward_listen,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_methods_negotiated",
        builtin_ssh2_methods_negotiated,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_publickey_add",
        builtin_ssh2_publickey_add,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_publickey_init",
        builtin_ssh2_publickey_init,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_publickey_list",
        builtin_ssh2_publickey_list,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_publickey_remove",
        builtin_ssh2_publickey_remove,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_scp_recv",
        builtin_ssh2_scp_recv,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_scp_send",
        builtin_ssh2_scp_send,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("ssh2_sftp", builtin_ssh2_sftp, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ssh2_sftp_chmod",
        builtin_ssh2_sftp_false,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_sftp_lstat",
        builtin_ssh2_sftp_stat,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_sftp_mkdir",
        builtin_ssh2_sftp_false,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_sftp_readlink",
        builtin_ssh2_sftp_string_false,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_sftp_realpath",
        builtin_ssh2_sftp_realpath,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_sftp_rename",
        builtin_ssh2_sftp_false,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_sftp_rmdir",
        builtin_ssh2_sftp_false,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_sftp_stat",
        builtin_ssh2_sftp_stat,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_sftp_symlink",
        builtin_ssh2_sftp_false,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ssh2_sftp_unlink",
        builtin_ssh2_sftp_false,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("ssh2_shell", builtin_ssh2_shell, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ssh2_tunnel",
        builtin_ssh2_tunnel,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_ssh2_connect(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=4).contains(&args.len()) {
        return Err(arity_error("ssh2_connect", "one to four arguments"));
    }
    let host = string_arg("ssh2_connect", &args[0])?
        .to_string_lossy()
        .to_owned();
    let port = optional_int("ssh2_connect", args.get(1), 22)?;
    if let Some(value) = args.get(2)
        && !matches!(value, Value::Array(_) | Value::Null)
    {
        return Err(argument_type_error(
            "ssh2_connect",
            "#3 ($methods)",
            "array|null",
            value,
        ));
    }
    if let Some(value) = args.get(3)
        && !matches!(value, Value::Array(_) | Value::Null)
    {
        return Err(argument_type_error(
            "ssh2_connect",
            "#4 ($callbacks)",
            "array|null",
            value,
        ));
    }
    let id = context.ssh2_state().connect(host, port);
    if live_ssh2_endpoint_enabled(context, &ssh2_endpoint(&args[0], port)?)
        && !context.ssh2_state().connect_backend(id)
    {
        let _ = context.ssh2_state().close(id);
        return Ok(Value::Bool(false));
    }
    Ok(Value::Object(ssh2_session_object(id)))
}

fn builtin_ssh2_disconnect(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let id = single_session_arg("ssh2_disconnect", &args)?;
    Ok(Value::Bool(context.ssh2_state().close(id)))
}

fn builtin_ssh2_auth_password(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("ssh2_auth_password", "exactly three arguments"));
    }
    let id = ssh2_session_id_arg("ssh2_auth_password", &args[0])?;
    let username = string_arg("ssh2_auth_password", &args[1])?
        .to_string_lossy()
        .to_owned();
    let password = string_arg("ssh2_auth_password", &args[2])?
        .to_string_lossy()
        .to_owned();
    if let Some(authenticated) = context
        .ssh2_state()
        .auth_password_backend(id, &username, &password)
    {
        return Ok(Value::Bool(authenticated));
    }
    context.ssh2_state().set_error(id, SSH2_BACKEND_ERROR);
    Ok(Value::Bool(false))
}

fn builtin_ssh2_auth_pubkey_file(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(4..=5).contains(&args.len()) {
        return Err(arity_error(
            "ssh2_auth_pubkey_file",
            "four or five arguments",
        ));
    }
    let id = ssh2_session_id_arg("ssh2_auth_pubkey_file", &args[0])?;
    let username = string_arg("ssh2_auth_pubkey_file", &args[1])?
        .to_string_lossy()
        .to_owned();
    let pubkey = PathBuf::from(
        string_arg("ssh2_auth_pubkey_file", &args[2])?
            .to_string_lossy()
            .to_owned(),
    );
    let privatekey = PathBuf::from(
        string_arg("ssh2_auth_pubkey_file", &args[3])?
            .to_string_lossy()
            .to_owned(),
    );
    let passphrase = match args.get(4) {
        Some(value) => Some(
            string_arg("ssh2_auth_pubkey_file", value)?
                .to_string_lossy()
                .to_owned(),
        ),
        None => None,
    };
    if let Some(authenticated) = context.ssh2_state().auth_pubkey_file_backend(
        id,
        &username,
        &pubkey,
        &privatekey,
        passphrase.as_deref(),
    ) {
        return Ok(Value::Bool(authenticated));
    }
    context.ssh2_state().set_error(id, SSH2_BACKEND_ERROR);
    Ok(Value::Bool(false))
}

fn builtin_ssh2_auth_hostbased_file(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(5..=6).contains(&args.len()) {
        return Err(arity_error(
            "ssh2_auth_hostbased_file",
            "five or six arguments",
        ));
    }
    let id = ssh2_session_id_arg("ssh2_auth_hostbased_file", &args[0])?;
    for value in args.iter().skip(1) {
        let _ = string_arg("ssh2_auth_hostbased_file", value)?;
    }
    context.ssh2_state().set_error(id, SSH2_BACKEND_ERROR);
    Ok(Value::Bool(false))
}

fn builtin_ssh2_auth_none(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ssh2_auth_none", "exactly two arguments"));
    }
    let id = ssh2_session_id_arg("ssh2_auth_none", &args[0])?;
    let _username = string_arg("ssh2_auth_none", &args[1])?;
    context.ssh2_state().set_error(id, SSH2_BACKEND_ERROR);
    let mut methods = PhpArray::new();
    methods.insert(ArrayKey::Int(0), Value::string("password"));
    methods.insert(ArrayKey::Int(1), Value::string("publickey"));
    Ok(Value::Array(methods))
}

fn builtin_ssh2_exec(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=6).contains(&args.len()) {
        return Err(arity_error("ssh2_exec", "two to six arguments"));
    }
    let id = ssh2_session_id_arg("ssh2_exec", &args[0])?;
    let command = string_arg("ssh2_exec", &args[1])?
        .to_string_lossy()
        .to_owned();
    validate_optional_strings("ssh2_exec", &args[2..])?;
    if context.ssh2_state().has_backend(id) {
        let Some(output) = context.ssh2_state().exec_backend(id, &command) else {
            return Ok(Value::Bool(false));
        };
        return ssh2_stream_resource(context, id, output);
    }
    context.ssh2_state().set_error(id, SSH2_BACKEND_ERROR);
    Ok(Value::Bool(false))
}

fn builtin_ssh2_shell(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=6).contains(&args.len()) {
        return Err(arity_error("ssh2_shell", "one to six arguments"));
    }
    let id = ssh2_session_id_arg("ssh2_shell", &args[0])?;
    validate_optional_strings("ssh2_shell", &args[1..])?;
    context.ssh2_state().set_error(id, SSH2_BACKEND_ERROR);
    Ok(Value::Bool(false))
}

fn builtin_ssh2_tunnel(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("ssh2_tunnel", "exactly three arguments"));
    }
    let id = ssh2_session_id_arg("ssh2_tunnel", &args[0])?;
    let _host = string_arg("ssh2_tunnel", &args[1])?;
    let _port = int_arg("ssh2_tunnel", &args[2])?;
    context.ssh2_state().set_error(id, SSH2_BACKEND_ERROR);
    Ok(Value::Bool(false))
}

fn builtin_ssh2_sftp(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let id = single_session_arg("ssh2_sftp", &args)?;
    let Some(sftp_id) = context.ssh2_state().sftp(id) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Object(ssh2_sftp_object(sftp_id)))
}

fn builtin_ssh2_sftp_realpath(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ssh2_sftp_realpath", "exactly two arguments"));
    }
    let id = ssh2_sftp_id_arg("ssh2_sftp_realpath", &args[0])?;
    let path = string_arg("ssh2_sftp_realpath", &args[1])?;
    if !context.ssh2_state().sftp_is_open(id) {
        return Ok(Value::Bool(false));
    }
    Ok(Value::String(path))
}

fn builtin_ssh2_sftp_stat(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ssh2_sftp_stat", "exactly two arguments"));
    }
    let id = ssh2_sftp_id_arg("ssh2_sftp_stat", &args[0])?;
    let _path = string_arg("ssh2_sftp_stat", &args[1])?;
    if !context.ssh2_state().sftp_is_open(id) {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(false))
}

fn builtin_ssh2_sftp_false(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 {
        return Err(arity_error("ssh2_sftp operation", "at least two arguments"));
    }
    let id = ssh2_sftp_id_arg("ssh2_sftp operation", &args[0])?;
    let _path = string_arg("ssh2_sftp operation", &args[1])?;
    if !context.ssh2_state().sftp_is_open(id) {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(false))
}

fn builtin_ssh2_sftp_string_false(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    builtin_ssh2_sftp_false(context, args, RuntimeSourceSpan::default())
}

fn builtin_ssh2_scp_recv(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("ssh2_scp_recv", "exactly three arguments"));
    }
    let id = ssh2_session_id_arg("ssh2_scp_recv", &args[0])?;
    let remote = PathBuf::from(
        string_arg("ssh2_scp_recv", &args[1])?
            .to_string_lossy()
            .to_owned(),
    );
    let local = PathBuf::from(
        string_arg("ssh2_scp_recv", &args[2])?
            .to_string_lossy()
            .to_owned(),
    );
    if let Some(copied) = context.ssh2_state().scp_recv_backend(id, &remote, &local) {
        return Ok(Value::Bool(copied));
    }
    context.ssh2_state().set_error(id, SSH2_BACKEND_ERROR);
    Ok(Value::Bool(false))
}

fn builtin_ssh2_scp_send(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=5).contains(&args.len()) {
        return Err(arity_error("ssh2_scp_send", "three to five arguments"));
    }
    let id = ssh2_session_id_arg("ssh2_scp_send", &args[0])?;
    let local = PathBuf::from(
        string_arg("ssh2_scp_send", &args[1])?
            .to_string_lossy()
            .to_owned(),
    );
    let remote = PathBuf::from(
        string_arg("ssh2_scp_send", &args[2])?
            .to_string_lossy()
            .to_owned(),
    );
    let mode = match args.get(3) {
        Some(value) => i32::try_from(int_arg("ssh2_scp_send", value)?).unwrap_or(0o644),
        None => 0o644,
    };
    if let Some(value) = args.get(4) {
        let _ = int_arg("ssh2_scp_send", value)?;
    }
    if let Some(copied) = context
        .ssh2_state()
        .scp_send_backend(id, &local, &remote, mode)
    {
        return Ok(Value::Bool(copied));
    }
    context.ssh2_state().set_error(id, SSH2_BACKEND_ERROR);
    Ok(Value::Bool(false))
}

fn builtin_ssh2_fingerprint(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("ssh2_fingerprint", "one or two arguments"));
    }
    let id = ssh2_session_id_arg("ssh2_fingerprint", &args[0])?;
    let flags = optional_int("ssh2_fingerprint", args.get(1), 0)?;
    if !context.ssh2_state().is_open(id) {
        return Ok(Value::Bool(false));
    }
    let hash = if flags & 1 == 1 {
        Ssh2FingerprintHash::Sha1
    } else {
        Ssh2FingerprintHash::Md5
    };
    if let Some(bytes) = context.ssh2_state().fingerprint_backend(id, hash) {
        if flags & 2 == 2 {
            return Ok(Value::String(PhpString::from_bytes(bytes)));
        }
        return Ok(Value::string(hex_bytes(&bytes)));
    }
    Ok(Value::string(""))
}

fn builtin_ssh2_methods_negotiated(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let id = single_session_arg("ssh2_methods_negotiated", &args)?;
    if !context.ssh2_state().is_open(id) {
        return Ok(Value::Bool(false));
    }
    let mut methods = PhpArray::new();
    for key in [
        "kex",
        "hostkey",
        "client_to_server",
        "server_to_client",
        "crypt_cs",
        "crypt_sc",
        "mac_cs",
        "mac_sc",
        "comp_cs",
        "comp_sc",
        "lang_cs",
        "lang_sc",
    ] {
        methods.insert(ArrayKey::String(key.into()), Value::string(""));
    }
    Ok(Value::Array(methods))
}

fn builtin_ssh2_publickey_init(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let id = single_session_arg("ssh2_publickey_init", &args)?;
    context.ssh2_state().set_error(id, SSH2_BACKEND_ERROR);
    Ok(Value::Bool(false))
}

fn builtin_ssh2_publickey_add(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 3 {
        return Err(arity_error(
            "ssh2_publickey_add",
            "at least three arguments",
        ));
    }
    Ok(Value::Bool(false))
}

fn builtin_ssh2_publickey_remove(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error(
            "ssh2_publickey_remove",
            "exactly two arguments",
        ));
    }
    Ok(Value::Bool(false))
}

fn builtin_ssh2_publickey_list(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ssh2_publickey_list", "exactly one argument"));
    }
    Ok(Value::Bool(false))
}

fn builtin_ssh2_forward_listen(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error("ssh2_forward_listen", "two to four arguments"));
    }
    let id = ssh2_session_id_arg("ssh2_forward_listen", &args[0])?;
    for value in args.iter().skip(1) {
        match value {
            Value::String(_) => {
                let _ = string_arg("ssh2_forward_listen", value)?;
            }
            _ => {
                let _ = int_arg("ssh2_forward_listen", value)?;
            }
        }
    }
    context.ssh2_state().set_error(id, SSH2_BACKEND_ERROR);
    Ok(Value::Bool(false))
}

fn builtin_ssh2_forward_accept(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ssh2_forward_accept", "exactly one argument"));
    }
    Ok(Value::Bool(false))
}

fn single_session_arg(function: &'static str, args: &[Value]) -> Result<i64, BuiltinError> {
    if args.len() != 1 {
        return Err(arity_error(function, "exactly one argument"));
    }
    ssh2_session_id_arg(function, &args[0])
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

fn validate_optional_strings(function: &'static str, values: &[Value]) -> Result<(), BuiltinError> {
    for value in values {
        match value {
            Value::Null | Value::Array(_) => {}
            Value::Int(_) | Value::Float(_) | Value::Bool(_) | Value::String(_) => {
                let _ = string_arg(function, value)?;
            }
            _ => {
                return Err(argument_type_error(
                    function,
                    "optional",
                    "string|array|null",
                    value,
                ));
            }
        }
    }
    Ok(())
}

fn ssh2_endpoint(host_value: &Value, port: i64) -> Result<String, BuiltinError> {
    let host = string_arg("ssh2_connect", host_value)?
        .to_string_lossy()
        .to_owned();
    Ok(format!("{host}:{port}"))
}

fn live_ssh2_endpoint_enabled(context: &BuiltinContext<'_>, endpoint: &str) -> bool {
    context.network_requests_enabled()
        && context
            .env_value("PHRUST_SSH2_LIVE_ENDPOINT")
            .is_some_and(|expected| expected == endpoint)
}

fn ssh2_stream_resource(
    context: &mut BuiltinContext<'_>,
    session_id: i64,
    output: Vec<u8>,
) -> BuiltinResult {
    let Some(resources) = context.resources() else {
        context
            .ssh2_state()
            .set_error(session_id, "SSH2 stream resource table is unavailable");
        return Ok(Value::Bool(false));
    };
    let resource = resources.register_stream(
        StreamFlags::new(true, true, true),
        StreamMetadata::new("ssh2", "stream", "r+", format!("ssh2.exec://{session_id}")),
    );
    if let Err(error) = resource.write_bytes(&output) {
        context.ssh2_state().set_error(session_id, error.message());
        return Ok(Value::Bool(false));
    }
    if let Err(error) = resource.rewind() {
        context.ssh2_state().set_error(session_id, error.message());
        return Ok(Value::Bool(false));
    }
    Ok(Value::Resource(resource))
}

fn hex_bytes(bytes: &[u8]) -> Vec<u8> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = Vec::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize]);
        out.push(HEX[(byte & 0x0f) as usize]);
    }
    out
}

fn ssh2_session_id_arg(function: &'static str, value: &Value) -> Result<i64, BuiltinError> {
    let Value::Object(object) = value else {
        return Err(argument_type_error(
            function,
            "#1",
            SSH2_SESSION_CLASS,
            value,
        ));
    };
    match object.get_property(SSH2_SESSION_ID_PROPERTY) {
        Some(Value::Int(id)) => Ok(id),
        _ => Err(argument_type_error(
            function,
            "#1",
            SSH2_SESSION_CLASS,
            value,
        )),
    }
}

fn ssh2_sftp_id_arg(function: &'static str, value: &Value) -> Result<i64, BuiltinError> {
    let Value::Object(object) = value else {
        return Err(argument_type_error(function, "#1", SSH2_SFTP_CLASS, value));
    };
    match object.get_property(SSH2_SFTP_ID_PROPERTY) {
        Some(Value::Int(id)) => Ok(id),
        _ => Err(argument_type_error(function, "#1", SSH2_SFTP_CLASS, value)),
    }
}

fn ssh2_session_object(id: i64) -> ObjectRef {
    let object =
        ObjectRef::new_with_display_name(&runtime_class("ssh2\\session"), SSH2_SESSION_CLASS);
    object.set_property(SSH2_SESSION_ID_PROPERTY, Value::Int(id));
    object
}

fn ssh2_sftp_object(id: i64) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&runtime_class("ssh2\\sftp"), SSH2_SFTP_CLASS);
    object.set_property(SSH2_SFTP_ID_PROPERTY, Value::Int(id));
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
    fn ssh2_facade_models_session_and_sftp_handles() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let session = builtin_ssh2_connect(
            &mut context,
            vec![Value::string("127.0.0.1")],
            RuntimeSourceSpan::default(),
        )
        .expect("connect succeeds");
        assert!(matches!(session, Value::Object(_)));
        assert_eq!(
            builtin_ssh2_auth_password(
                &mut context,
                vec![
                    session.clone(),
                    Value::string("user"),
                    Value::string("secret"),
                ],
                RuntimeSourceSpan::default(),
            )
            .expect("auth"),
            Value::Bool(false)
        );
        let sftp = builtin_ssh2_sftp(
            &mut context,
            vec![session.clone()],
            RuntimeSourceSpan::default(),
        )
        .expect("sftp");
        assert!(matches!(sftp, Value::Object(_)));
        assert_eq!(
            builtin_ssh2_fingerprint(
                &mut context,
                vec![session.clone()],
                RuntimeSourceSpan::default(),
            )
            .expect("fingerprint"),
            Value::string("")
        );
    }
}
