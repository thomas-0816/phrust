//! Loopback-only TCP subset for the sockets extension.

use super::core::{argument_type_error, arity_error, int_arg, string_arg};
use crate::builtins::{BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult};
use crate::{
    BuiltinError, ClassEntry, ClassFlags, ObjectRef, RuntimeSourceSpan, Value, normalize_class_name,
};
use std::convert::TryFrom;
use std::io;

const SOCKET_CLASS: &str = "Socket";
const SOCKET_ID_PROPERTY: &str = "__phrust_socket_id";

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("inet_ntop", builtin_inet_ntop, BuiltinCompatibility::Php),
    BuiltinEntry::new("inet_pton", builtin_inet_pton, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "socket_accept",
        builtin_socket_accept,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_bind",
        builtin_socket_bind,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_clear_error",
        builtin_socket_clear_error,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_close",
        builtin_socket_close,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_connect",
        builtin_socket_connect,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_create",
        builtin_socket_create,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_getsockname",
        builtin_socket_getsockname,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_getpeername",
        builtin_socket_getpeername,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_recv",
        builtin_socket_recv,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_send",
        builtin_socket_send,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_shutdown",
        builtin_socket_shutdown,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_last_error",
        builtin_socket_last_error,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_listen",
        builtin_socket_listen,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_read",
        builtin_socket_read,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_strerror",
        builtin_socket_strerror,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_write",
        builtin_socket_write,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_socket_create(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("socket_create", "three arguments"));
    }
    let domain = int_arg("socket_create", &args[0])?;
    let socket_type = int_arg("socket_create", &args[1])?;
    let protocol = int_arg("socket_create", &args[2])?;
    if domain != i64::from(libc::AF_INET)
        || socket_type != i64::from(libc::SOCK_STREAM)
        || (protocol != 0 && protocol != i64::from(libc::IPPROTO_TCP))
    {
        context.socket_state().set_last_error(libc::EAFNOSUPPORT);
        return Ok(Value::Bool(false));
    }
    let id = context.socket_state().create(domain, socket_type, protocol);
    Ok(Value::Object(socket_object(id)))
}

fn builtin_socket_bind(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("socket_bind", "two or three arguments"));
    }
    let socket_id = socket_id_arg("socket_bind", &args[0])?;
    let address = string_arg("socket_bind", &args[1])?.to_string();
    let port = if let Some(value) = args.get(2) {
        port_arg("socket_bind", value)?
    } else {
        0
    };
    match context
        .socket_state()
        .bind_tcp_listener(socket_id, &address, port)
    {
        Ok(()) => Ok(Value::Bool(true)),
        Err(errno) => {
            context.socket_state().set_last_error(errno);
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_socket_listen(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("socket_listen", "one or two arguments"));
    }
    let socket_id = socket_id_arg("socket_listen", &args[0])?;
    if let Some(value) = args.get(1) {
        let _ = int_arg("socket_listen", value)?;
    }
    match context.socket_state().listen(socket_id) {
        Ok(()) => Ok(Value::Bool(true)),
        Err(errno) => {
            context.socket_state().set_last_error(errno);
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_socket_connect(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("socket_connect", "two or three arguments"));
    }
    let socket_id = socket_id_arg("socket_connect", &args[0])?;
    let address = string_arg("socket_connect", &args[1])?.to_string();
    let port = if let Some(value) = args.get(2) {
        port_arg("socket_connect", value)?
    } else {
        0
    };
    match context
        .socket_state()
        .connect_tcp(socket_id, &address, port)
    {
        Ok(()) => Ok(Value::Bool(true)),
        Err(errno) => {
            context.socket_state().set_last_error(errno);
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_socket_accept(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("socket_accept", "one argument"));
    }
    let socket_id = socket_id_arg("socket_accept", &args[0])?;
    match context.socket_state().accept(socket_id) {
        Ok(accepted_id) => Ok(Value::Object(socket_object(accepted_id))),
        Err(errno) => {
            context.socket_state().set_last_error(errno);
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_socket_write(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("socket_write", "two or three arguments"));
    }
    let socket_id = socket_id_arg("socket_write", &args[0])?;
    let bytes = string_arg("socket_write", &args[1])?.as_bytes().to_vec();
    let length = if let Some(value) = args.get(2) {
        usize::try_from(int_arg("socket_write", value)?)
            .map_err(|_| {
                BuiltinError::new(
                    "E_PHP_RUNTIME_VALUE",
                    "socket_write(): length must be non-negative",
                )
            })?
            .min(bytes.len())
    } else {
        bytes.len()
    };
    match context.socket_state().write(socket_id, &bytes[..length]) {
        Ok(written) => Ok(Value::Int(written as i64)),
        Err(errno) => {
            context.socket_state().set_last_error(errno);
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_socket_read(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("socket_read", "two or three arguments"));
    }
    let socket_id = socket_id_arg("socket_read", &args[0])?;
    let length = usize::try_from(int_arg("socket_read", &args[1])?).map_err(|_| {
        BuiltinError::new(
            "E_PHP_RUNTIME_VALUE",
            "socket_read(): length must be non-negative",
        )
    })?;
    if let Some(value) = args.get(2) {
        let _ = int_arg("socket_read", value)?;
    }
    match context.socket_state().read(socket_id, length) {
        Ok(bytes) => Ok(Value::string(bytes)),
        Err(errno) => {
            context.socket_state().set_last_error(errno);
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_socket_send(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 4 {
        return Err(arity_error("socket_send", "four arguments"));
    }
    let socket_id = socket_id_arg("socket_send", &args[0])?;
    let bytes = string_arg("socket_send", &args[1])?.as_bytes().to_vec();
    let length = usize::try_from(int_arg("socket_send", &args[2])?).map_err(|_| {
        BuiltinError::new(
            "E_PHP_RUNTIME_VALUE",
            "socket_send(): length must be non-negative",
        )
    })?;
    let _flags = int_arg("socket_send", &args[3])?;
    match context
        .socket_state()
        .write(socket_id, &bytes[..length.min(bytes.len())])
    {
        Ok(written) => Ok(Value::Int(written as i64)),
        Err(errno) => {
            context.socket_state().set_last_error(errno);
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_socket_recv(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 4 {
        return Err(arity_error("socket_recv", "four arguments"));
    }
    let socket_id = socket_id_arg("socket_recv", &args[0])?;
    let length = usize::try_from(int_arg("socket_recv", &args[2])?).map_err(|_| {
        BuiltinError::new(
            "E_PHP_RUNTIME_VALUE",
            "socket_recv(): length must be non-negative",
        )
    })?;
    let _flags = int_arg("socket_recv", &args[3])?;
    match context.socket_state().read(socket_id, length) {
        Ok(bytes) => {
            if let Value::Reference(cell) = &args[1] {
                cell.set(Value::string(bytes.clone()));
            }
            Ok(Value::Int(bytes.len() as i64))
        }
        Err(errno) => {
            context.socket_state().set_last_error(errno);
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_socket_getsockname(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("socket_getsockname", "two or three arguments"));
    }
    let socket_id = socket_id_arg("socket_getsockname", &args[0])?;
    let Some(addr) = context.socket_state().local_addr(socket_id) else {
        context.socket_state().set_last_error(libc::EINVAL);
        return Ok(Value::Bool(false));
    };
    if let Value::Reference(cell) = &args[1] {
        cell.set(Value::string(addr.ip().to_string()));
    }
    if let Some(Value::Reference(cell)) = args.get(2) {
        cell.set(Value::Int(i64::from(addr.port())));
    }
    Ok(Value::Bool(true))
}

fn builtin_socket_getpeername(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("socket_getpeername", "two or three arguments"));
    }
    let socket_id = socket_id_arg("socket_getpeername", &args[0])?;
    let Some(addr) = context.socket_state().peer_addr(socket_id) else {
        context.socket_state().set_last_error(libc::EINVAL);
        return Ok(Value::Bool(false));
    };
    if let Value::Reference(cell) = &args[1] {
        cell.set(Value::string(addr.ip().to_string()));
    }
    if let Some(Value::Reference(cell)) = args.get(2) {
        cell.set(Value::Int(i64::from(addr.port())));
    }
    Ok(Value::Bool(true))
}

fn builtin_socket_shutdown(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 2 || args.is_empty() {
        return Err(arity_error("socket_shutdown", "one or two arguments"));
    }
    let socket_id = socket_id_arg("socket_shutdown", &args[0])?;
    let mode = if let Some(value) = args.get(1) {
        int_arg("socket_shutdown", value)?
    } else {
        2
    };
    match context.socket_state().shutdown(socket_id, mode) {
        Ok(()) => Ok(Value::Bool(true)),
        Err(errno) => {
            context.socket_state().set_last_error(errno);
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_socket_close(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("socket_close", "one argument"));
    }
    let socket_id = socket_id_arg("socket_close", &args[0])?;
    if let Err(errno) = context.socket_state().close(socket_id) {
        context.socket_state().set_last_error(errno);
    }
    Ok(Value::Null)
}

fn builtin_socket_last_error(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("socket_last_error", "zero or one argument"));
    }
    if let Some(value) = args.first() {
        let _ = socket_id_arg("socket_last_error", value)?;
    }
    Ok(Value::Int(i64::from(context.socket_state().last_error())))
}

fn builtin_socket_clear_error(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("socket_clear_error", "zero or one argument"));
    }
    if let Some(value) = args.first()
        && !matches!(value, Value::Null)
    {
        let _ = socket_id_arg("socket_clear_error", value)?;
    }
    context.socket_state().set_last_error(0);
    Ok(Value::Null)
}

fn builtin_socket_strerror(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("socket_strerror", "one argument"));
    }
    let code = int_arg("socket_strerror", &args[0])?;
    if code == 0 {
        return Ok(Value::string("Success"));
    }
    let code = i32::try_from(code).unwrap_or(libc::EINVAL);
    Ok(Value::string(
        io::Error::from_raw_os_error(code).to_string(),
    ))
}

fn builtin_inet_pton(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("inet_pton", "one argument"));
    }
    let address = string_arg("inet_pton", &args[0])?;
    let Ok(address) = std::str::from_utf8(address.as_bytes()) else {
        return Ok(Value::Bool(false));
    };
    match address.parse::<std::net::IpAddr>() {
        Ok(addr) => Ok(Value::string(match addr {
            std::net::IpAddr::V4(addr) => addr.octets().to_vec(),
            std::net::IpAddr::V6(addr) => addr.octets().to_vec(),
        })),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_inet_ntop(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("inet_ntop", "one argument"));
    }
    let packed = string_arg("inet_ntop", &args[0])?;
    let packed = packed.as_bytes();
    match packed.len() {
        4 => Ok(Value::string(
            std::net::Ipv4Addr::new(packed[0], packed[1], packed[2], packed[3]).to_string(),
        )),
        16 => {
            let mut octets = [0; 16];
            octets.copy_from_slice(packed);
            Ok(Value::string(std::net::Ipv6Addr::from(octets).to_string()))
        }
        _ => Ok(Value::Bool(false)),
    }
}

fn socket_id_arg(name: &str, value: &Value) -> Result<i64, BuiltinError> {
    let Value::Object(object) = value else {
        return Err(argument_type_error(
            name,
            "#1 ($socket)",
            SOCKET_CLASS,
            value,
        ));
    };
    if normalize_class_name(&object.class_name()) != "socket" {
        return Err(argument_type_error(
            name,
            "#1 ($socket)",
            SOCKET_CLASS,
            value,
        ));
    }
    match object.get_property(SOCKET_ID_PROPERTY) {
        Some(Value::Int(id)) if id > 0 => Ok(id),
        _ => Err(BuiltinError::new(
            "E_PHP_RUNTIME_SOCKET_INVALID",
            format!("{name}(): Socket object is no longer valid"),
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

fn socket_object(id: i64) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&socket_runtime_class(), SOCKET_CLASS);
    object.set_property(SOCKET_ID_PROPERTY, Value::Int(id));
    object
}

fn socket_runtime_class() -> ClassEntry {
    ClassEntry {
        name: "socket".to_owned(),
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
