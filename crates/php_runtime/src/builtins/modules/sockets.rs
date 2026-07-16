//! Loopback-only TCP subset for the sockets extension.

use super::core::{argument_type_error, arity_error, int_arg, string_arg};
use crate::builtins::{BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult};
use crate::{
    ArrayKey, BuiltinError, ClassEntry, ClassFlags, ObjectRef, PhpArray, PhpString,
    RuntimeSourceSpan, Value, normalize_class_name,
};
use std::convert::TryFrom;
use std::io;

const SOCKET_CLASS: &str = "Socket";
const SOCKET_ID_PROPERTY: &str = "__phrust_socket_id";

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("inet_ntop", builtin_inet_ntop, BuiltinCompatibility::Php),
    BuiltinEntry::new("inet_pton", builtin_inet_pton, BuiltinCompatibility::Php),
    BuiltinEntry::new("ip2long", builtin_ip2long, BuiltinCompatibility::Php),
    BuiltinEntry::new("long2ip", builtin_long2ip, BuiltinCompatibility::Php),
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
        "socket_get_option",
        builtin_socket_get_option,
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
        "socket_sendmsg",
        builtin_socket_sendmsg,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_select",
        builtin_socket_select,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_set_nonblock",
        builtin_socket_set_nonblock,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "socket_set_option",
        builtin_socket_set_option,
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
    let tcp_stream = domain == i64::from(libc::AF_INET)
        && socket_type == i64::from(libc::SOCK_STREAM)
        && (protocol == 0 || protocol == i64::from(libc::IPPROTO_TCP));
    let udp_datagram = domain == i64::from(libc::AF_INET)
        && socket_type == i64::from(libc::SOCK_DGRAM)
        && (protocol == 0 || protocol == i64::from(libc::IPPROTO_UDP));
    #[cfg(unix)]
    let unix_stream = domain == i64::from(libc::AF_UNIX)
        && socket_type == i64::from(libc::SOCK_STREAM)
        && protocol == 0;
    #[cfg(unix)]
    let unix_datagram = domain == i64::from(libc::AF_UNIX)
        && socket_type == i64::from(libc::SOCK_DGRAM)
        && protocol == 0;
    #[cfg(not(unix))]
    let unix_stream = false;
    #[cfg(not(unix))]
    let unix_datagram = false;
    if !tcp_stream && !udp_datagram && !unix_stream && !unix_datagram {
        context.socket_state().set_last_error(libc::EAFNOSUPPORT);
        return Ok(Value::Bool(false));
    }
    match context.socket_state().create(domain, socket_type, protocol) {
        Ok(id) => Ok(Value::Object(socket_object(id))),
        Err(errno) => {
            context.socket_state().set_last_error(errno);
            Ok(Value::Bool(false))
        }
    }
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
        .bind_stream_listener(socket_id, &address, port)
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
    let backlog = args
        .get(1)
        .map(|value| int_arg("socket_listen", value))
        .transpose()?
        .unwrap_or(i64::from(i32::MAX));
    let backlog = i32::try_from(backlog).unwrap_or(i32::MAX);
    match context.socket_state().listen(socket_id, backlog) {
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
        .connect_stream(socket_id, &address, port)
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

fn builtin_socket_sendmsg(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("socket_sendmsg", "three arguments"));
    }
    let socket_id = socket_id_arg("socket_sendmsg", &args[0])?;
    let message = value_array("socket_sendmsg", "#2 ($message)", &args[1])?;
    let iov = message.get(&string_key("iov")).ok_or_else(|| {
        BuiltinError::new(
            "E_PHP_RUNTIME_SOCKET_MESSAGE",
            "socket_sendmsg(): Argument #2 ($message) must contain an iov key",
        )
    })?;
    let iov = value_array("socket_sendmsg", "iov", iov)?;
    let mut bytes = Vec::new();
    for (_, value) in iov.iter() {
        bytes.extend_from_slice(string_arg("socket_sendmsg", value)?.as_bytes());
    }
    let address = message
        .get(&string_key("name"))
        .map(|name| value_array("socket_sendmsg", "name", name))
        .transpose()?
        .and_then(|name| name.get(&string_key("path")).cloned())
        .map(|path| string_arg("socket_sendmsg", &path).map(|path| path.to_string()))
        .transpose()?;
    let _flags = int_arg("socket_sendmsg", &args[2])?;
    match context
        .socket_state()
        .send_message(socket_id, &bytes, address.as_deref())
    {
        Ok(written) => Ok(Value::Int(written as i64)),
        Err(errno) => {
            context.socket_state().set_last_error(errno);
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_socket_set_nonblock(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("socket_set_nonblock", "one argument"));
    }
    let socket_id = socket_id_arg("socket_set_nonblock", &args[0])?;
    match context.socket_state().set_nonblocking(socket_id, true) {
        Ok(()) => Ok(Value::Bool(true)),
        Err(errno) => {
            context.socket_state().set_last_error(errno);
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_socket_select(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(4..=5).contains(&args.len()) {
        return Err(arity_error("socket_select", "four or five arguments"));
    }
    let read = value_array("socket_select", "#1 ($read)", &args[0])?;
    let write = value_array("socket_select", "#2 ($write)", &args[1])?;
    let except = value_array("socket_select", "#3 ($except)", &args[2])?;
    if !write.is_empty() || !except.is_empty() {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_SOCKET_SELECT_WRITE_UNSUPPORTED",
            "socket_select(): write and except descriptor polling is not implemented",
        ));
    }
    let seconds = int_arg("socket_select", &args[3])?.max(0) as u64;
    let microseconds = args
        .get(4)
        .map(|value| int_arg("socket_select", value))
        .transpose()?
        .unwrap_or(0)
        .max(0) as u64;
    let timeout = std::time::Duration::from_secs(seconds)
        .saturating_add(std::time::Duration::from_micros(microseconds));
    let mut entries = Vec::with_capacity(read.len());
    for (key, value) in read.iter() {
        entries.push((key, socket_id_arg("socket_select", value)?, value.clone()));
    }
    let ids = entries.iter().map(|(_, id, _)| *id).collect::<Vec<_>>();
    let ready = match context.socket_state().poll_readable(&ids, timeout) {
        Ok(ready) => ready,
        Err(errno) => {
            context.socket_state().set_last_error(errno);
            return Ok(Value::Bool(false));
        }
    };
    let mut selected = PhpArray::new();
    for (key, id, value) in entries {
        if ready.contains(&id) {
            selected.insert(key, value);
        }
    }
    if let Value::Reference(reference) = &args[0] {
        reference.set(Value::Array(selected));
    }
    Ok(Value::Int(ready.len() as i64))
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
    let Some((address, port)) = context.socket_state().local_name(socket_id) else {
        context.socket_state().set_last_error(libc::EINVAL);
        return Ok(Value::Bool(false));
    };
    if let Value::Reference(cell) = &args[1] {
        cell.set(Value::string(address));
    }
    if let Some(Value::Reference(cell)) = args.get(2)
        && let Some(port) = port
    {
        cell.set(Value::Int(i64::from(port)));
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
    let Some((address, port)) = context.socket_state().peer_name(socket_id) else {
        context.socket_state().set_last_error(libc::EINVAL);
        return Ok(Value::Bool(false));
    };
    if let Value::Reference(cell) = &args[1] {
        cell.set(Value::string(address));
    }
    if let Some(Value::Reference(cell)) = args.get(2)
        && let Some(port) = port
    {
        cell.set(Value::Int(i64::from(port)));
    }
    Ok(Value::Bool(true))
}

fn builtin_socket_set_option(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 4 {
        return Err(arity_error("socket_set_option", "four arguments"));
    }
    let socket_id = socket_id_arg("socket_set_option", &args[0])?;
    let level = int_arg("socket_set_option", &args[1])?;
    let option = int_arg("socket_set_option", &args[2])?;
    #[cfg(target_os = "linux")]
    if level == i64::from(libc::IPPROTO_IP)
        && matches!(
            option as i32,
            libc::MCAST_LEAVE_GROUP | libc::MCAST_LEAVE_SOURCE_GROUP
        )
        && !matches!(super::core::deref_value(&args[3]), Value::Array(_))
    {
        let option_name = if option as i32 == libc::MCAST_LEAVE_GROUP {
            "MCAST_LEAVE_GROUP"
        } else {
            "MCAST_LEAVE_SOURCE_GROUP"
        };
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!(
                "socket_set_option(): Argument #4 ($value) must be of type array when argument #3 ($option) is {option_name}, {} given",
                super::core::php_argument_type_name(&args[3])
            ),
        ));
    }
    let value = socket_option_int("socket_set_option", &args[3])?;
    match context
        .socket_state()
        .set_option(socket_id, level, option, value)
    {
        Ok(()) => Ok(Value::Bool(true)),
        Err(errno) => {
            context.socket_state().set_last_error(errno);
            let message = io::Error::from_raw_os_error(errno).to_string();
            let suffix = format!(" (os error {errno})");
            let message = message.strip_suffix(&suffix).unwrap_or(&message);
            context.php_warning(
                "E_PHP_RUNTIME_SOCKET_OPTION",
                format!("socket_set_option(): Unable to set socket option [{errno}]: {message}"),
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_socket_get_option(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("socket_get_option", "three arguments"));
    }
    let socket_id = socket_id_arg("socket_get_option", &args[0])?;
    let level = int_arg("socket_get_option", &args[1])?;
    let option = int_arg("socket_get_option", &args[2])?;
    match context.socket_state().option(socket_id, level, option) {
        Ok(value) => Ok(Value::Int(value)),
        Err(errno) => {
            context.socket_state().set_last_error(errno);
            Ok(Value::Bool(false))
        }
    }
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
    let message = io::Error::from_raw_os_error(code).to_string();
    let rust_suffix = format!(" (os error {code})");
    Ok(Value::string(
        message
            .strip_suffix(&rust_suffix)
            .unwrap_or(&message)
            .to_owned(),
    ))
}

fn builtin_ip2long(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ip2long", "one argument"));
    }
    let address = string_arg("ip2long", &args[0])?;
    let Ok(address) = std::str::from_utf8(address.as_bytes()) else {
        return Ok(Value::Bool(false));
    };
    let Some(octets) = parse_php_ipv4(address) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Int(i64::from(u32::from_be_bytes(octets))))
}

fn builtin_long2ip(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("long2ip", "one argument"));
    }
    let address = int_arg("long2ip", &args[0])? as u32;
    Ok(Value::string(std::net::Ipv4Addr::from(address).to_string()))
}

fn parse_php_ipv4(address: &str) -> Option<[u8; 4]> {
    let mut octets = [0_u8; 4];
    let mut parts = address.split('.');
    for octet in &mut octets {
        let part = parts.next()?;
        if part.is_empty()
            || (part.len() > 1 && part.starts_with('0'))
            || !part.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        *octet = part.parse().ok()?;
    }
    parts.next().is_none().then_some(octets)
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

fn socket_option_int(name: &str, value: &Value) -> Result<i64, BuiltinError> {
    match value {
        Value::Bool(value) => Ok(i64::from(*value)),
        _ => int_arg(name, value),
    }
}

fn string_key(value: &str) -> ArrayKey {
    ArrayKey::String(PhpString::from(value))
}

fn value_array(name: &str, argument: &str, value: &Value) -> Result<PhpArray, BuiltinError> {
    match super::core::deref_value(value) {
        Value::Array(array) => Ok(array),
        value => Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!(
                "{name}(): Argument {argument} must be of type array, {} given",
                super::core::php_argument_type_name(&value)
            ),
        )),
    }
}

fn socket_object(id: i64) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&socket_runtime_class(), SOCKET_CLASS);
    object.set_property(SOCKET_ID_PROPERTY, Value::Int(id));
    object
}

fn socket_runtime_class() -> ClassEntry {
    ClassEntry {
        name: "socket".to_owned().into(),
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
    use super::parse_php_ipv4;

    #[test]
    fn php_ipv4_parser_accepts_only_four_canonical_decimal_octets() {
        assert_eq!(parse_php_ipv4("127.0.0.1"), Some([127, 0, 0, 1]));
        assert_eq!(parse_php_ipv4("255.255.255.255"), Some([255; 4]));
        for rejected in [
            "127.1",
            "1.2.3",
            "01.2.3.4",
            "1.2.3.004",
            "256.0.0.1",
            "192.168.0xa.5",
            "1.2.3.4 ",
            "",
        ] {
            assert_eq!(parse_php_ipv4(rejected), None, "accepted {rejected:?}");
        }
    }
}
