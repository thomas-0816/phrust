//! Bounded POSIX extension backed by host libc calls.

#![allow(unsafe_code)]

use super::core::{argument_value_error, arity_error, int_arg, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, RuntimeSourceSpan,
};
use crate::{ArrayKey, PhpArray, PhpString, Value};
use std::ffi::{CStr, CString};
use std::io;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "posix_access",
        builtin_posix_access,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_ctermid",
        builtin_posix_ctermid,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_eaccess",
        builtin_posix_eaccess,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_errno",
        builtin_posix_get_last_error,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_fpathconf",
        builtin_posix_false_gap,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_get_last_error",
        builtin_posix_get_last_error,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getcwd",
        builtin_posix_getcwd,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getegid",
        builtin_posix_getegid,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_geteuid",
        builtin_posix_geteuid,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getgid",
        builtin_posix_getgid,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getgrgid",
        builtin_posix_getgrgid,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getgrnam",
        builtin_posix_getgrnam,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getgroups",
        builtin_posix_getgroups,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getlogin",
        builtin_posix_getlogin,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getpgid",
        builtin_posix_getpgid,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getpgrp",
        builtin_posix_getpgrp,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getpid",
        builtin_posix_getpid,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getppid",
        builtin_posix_getppid,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getpwnam",
        builtin_posix_getpwnam,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getpwuid",
        builtin_posix_getpwuid,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getrlimit",
        builtin_posix_getrlimit,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getsid",
        builtin_posix_getsid,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_getuid",
        builtin_posix_getuid,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_initgroups",
        builtin_posix_false_gap,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_isatty",
        builtin_posix_isatty,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("posix_kill", builtin_posix_kill, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "posix_mkfifo",
        builtin_posix_mkfifo,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_mknod",
        builtin_posix_false_gap,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_pathconf",
        builtin_posix_pathconf,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_setegid",
        builtin_posix_false_gap,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_seteuid",
        builtin_posix_false_gap,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_setgid",
        builtin_posix_false_gap,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_setpgid",
        builtin_posix_false_gap,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_setrlimit",
        builtin_posix_false_gap,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_setsid",
        builtin_posix_setsid,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_setuid",
        builtin_posix_permission_gap,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_strerror",
        builtin_posix_strerror,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_sysconf",
        builtin_posix_sysconf,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_times",
        builtin_posix_times,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_ttyname",
        builtin_posix_ttyname,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "posix_uname",
        builtin_posix_uname,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_posix_getpid(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    no_args("posix_getpid", &args)?;
    Ok(Value::Int(unsafe { libc::getpid() as i64 }))
}

fn builtin_posix_getppid(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    no_args("posix_getppid", &args)?;
    Ok(Value::Int(unsafe { libc::getppid() as i64 }))
}

fn builtin_posix_getuid(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    no_args("posix_getuid", &args)?;
    Ok(Value::Int(unsafe { libc::getuid() as i64 }))
}

fn builtin_posix_geteuid(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    no_args("posix_geteuid", &args)?;
    Ok(Value::Int(unsafe { libc::geteuid() as i64 }))
}

fn builtin_posix_getgid(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    no_args("posix_getgid", &args)?;
    Ok(Value::Int(unsafe { libc::getgid() as i64 }))
}

fn builtin_posix_getegid(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    no_args("posix_getegid", &args)?;
    Ok(Value::Int(unsafe { libc::getegid() as i64 }))
}

fn builtin_posix_getpgrp(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    no_args("posix_getpgrp", &args)?;
    Ok(Value::Int(unsafe { libc::getpgrp() as i64 }))
}

fn builtin_posix_getpgid(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("posix_getpgid", "one argument"));
    }
    let pid = int_arg("posix_getpgid", &args[0])?;
    let result = unsafe { libc::getpgid(pid as libc::pid_t) };
    syscall_int_result(context, result)
}

fn builtin_posix_getsid(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("posix_getsid", "one argument"));
    }
    let pid = int_arg("posix_getsid", &args[0])?;
    if pid < 0 {
        return Err(argument_value_error(
            "posix_getsid",
            "#1 ($process_id)",
            &format!("must be between 0 and {}", i64::MAX),
        ));
    }
    let result = unsafe { libc::getsid(pid as libc::pid_t) };
    syscall_int_result(context, result)
}

fn builtin_posix_setsid(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    no_args("posix_setsid", &args)?;
    let result = unsafe { libc::setsid() };
    syscall_int_result(context, result)
}

fn builtin_posix_getcwd(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    no_args("posix_getcwd", &args)?;
    match std::env::current_dir() {
        Ok(path) => Ok(Value::string(path.to_string_lossy().into_owned())),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_posix_uname(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    no_args("posix_uname", &args)?;
    let mut uts = std::mem::MaybeUninit::<libc::utsname>::uninit();
    if unsafe { libc::uname(uts.as_mut_ptr()) } != 0 {
        context.set_posix_last_error(last_os_error());
        return Ok(Value::Bool(false));
    }
    let uts = unsafe { uts.assume_init() };
    let mut array = PhpArray::new();
    array.insert(
        string_key("sysname"),
        Value::string(uts_field(&uts.sysname)),
    );
    array.insert(
        string_key("nodename"),
        Value::string(uts_field(&uts.nodename)),
    );
    array.insert(
        string_key("release"),
        Value::string(uts_field(&uts.release)),
    );
    array.insert(
        string_key("version"),
        Value::string(uts_field(&uts.version)),
    );
    array.insert(
        string_key("machine"),
        Value::string(uts_field(&uts.machine)),
    );
    Ok(Value::Array(array))
}

fn builtin_posix_kill(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("posix_kill", "two arguments"));
    }
    let pid = int_arg("posix_kill", &args[0])?;
    let signal = int_arg("posix_kill", &args[1])?;
    let result = unsafe { libc::kill(pid as libc::pid_t, signal as libc::c_int) };
    if result == 0 {
        context.set_posix_last_error(0);
        Ok(Value::Bool(true))
    } else {
        context.set_posix_last_error(last_os_error());
        Ok(Value::Bool(false))
    }
}

fn builtin_posix_access(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    posix_access_impl(context, "posix_access", args, false)
}

fn builtin_posix_eaccess(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    posix_access_impl(context, "posix_eaccess", args, true)
}

fn posix_access_impl(
    context: &mut BuiltinContext<'_>,
    name: &'static str,
    args: Vec<Value>,
    reject_invalid_filename: bool,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error(name, "one or two arguments"));
    }
    let path = string_arg(name, &args[0])?;
    if reject_invalid_filename && (path.is_empty() || path.len() > posix_path_max()) {
        return Err(argument_value_error(
            name,
            "#1 ($filename)",
            "must not be empty",
        ));
    }
    let mode = args
        .get(1)
        .map(|value| int_arg(name, value))
        .transpose()?
        .unwrap_or(libc::F_OK as i64) as libc::c_int;
    let Ok(c_path) = CString::new(path.as_bytes()) else {
        context.set_posix_last_error(libc::EINVAL);
        return Ok(Value::Bool(false));
    };
    let result = unsafe { libc::access(c_path.as_ptr(), mode) };
    if result == 0 {
        context.set_posix_last_error(0);
        Ok(Value::Bool(true))
    } else {
        context.set_posix_last_error(last_os_error());
        Ok(Value::Bool(false))
    }
}

fn posix_path_max() -> usize {
    #[cfg(unix)]
    {
        libc::PATH_MAX as usize
    }
    #[cfg(not(unix))]
    {
        4096
    }
}

fn builtin_posix_mkfifo(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("posix_mkfifo", "two arguments"));
    }
    let path = string_arg("posix_mkfifo", &args[0])?;
    let mode = int_arg("posix_mkfifo", &args[1])? as libc::mode_t;
    let Ok(c_path) = CString::new(path.as_bytes()) else {
        context.set_posix_last_error(libc::EINVAL);
        return Ok(Value::Bool(false));
    };
    let result = unsafe { libc::mkfifo(c_path.as_ptr(), mode) };
    if result == 0 {
        context.set_posix_last_error(0);
        Ok(Value::Bool(true))
    } else {
        context.set_posix_last_error(last_os_error());
        Ok(Value::Bool(false))
    }
}

fn builtin_posix_isatty(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("posix_isatty", "one argument"));
    }
    let Some(fd) = fd_arg("posix_isatty", &args[0]) else {
        context.set_posix_last_error(libc::ENOSYS);
        return Ok(Value::Bool(false));
    };
    let result = unsafe { libc::isatty(fd) };
    if result == 1 {
        context.set_posix_last_error(0);
        Ok(Value::Bool(true))
    } else {
        context.set_posix_last_error(last_os_error());
        Ok(Value::Bool(false))
    }
}

fn builtin_posix_ttyname(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("posix_ttyname", "one argument"));
    }
    let Some(fd) = fd_arg("posix_ttyname", &args[0]) else {
        context.set_posix_last_error(libc::ENOSYS);
        return Ok(Value::Bool(false));
    };
    let tty = unsafe { libc::ttyname(fd) };
    if tty.is_null() {
        context.set_posix_last_error(last_os_error());
        return Ok(Value::Bool(false));
    }
    context.set_posix_last_error(0);
    Ok(Value::string(c_string(tty)))
}

fn builtin_posix_pathconf(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("posix_pathconf", "two arguments"));
    }
    let path = string_arg("posix_pathconf", &args[0])?;
    if path.as_bytes().is_empty() {
        return Err(argument_value_error(
            "posix_pathconf",
            "#1 ($path)",
            "must not be empty",
        ));
    }
    let name = int_arg("posix_pathconf", &args[1])? as libc::c_int;
    let Ok(c_path) = CString::new(path.as_bytes()) else {
        context.set_posix_last_error(libc::EINVAL);
        return Ok(Value::Bool(false));
    };
    clear_errno();
    let result = unsafe { libc::pathconf(c_path.as_ptr(), name) };
    let error = last_os_error();
    if result >= 0 {
        context.set_posix_last_error(0);
        Ok(Value::Int(result as i64))
    } else if error == 0 {
        context.set_posix_last_error(0);
        Ok(Value::Int(-1))
    } else {
        context.set_posix_last_error(error);
        Ok(Value::Bool(false))
    }
}

fn builtin_posix_sysconf(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("posix_sysconf", "one argument"));
    }
    let name = int_arg("posix_sysconf", &args[0])? as libc::c_int;
    if name == -1 {
        context.set_posix_last_error(0);
        return Ok(Value::Int(-1));
    }
    clear_errno();
    let result = unsafe { libc::sysconf(name) };
    let error = last_os_error();
    if result >= 0 {
        context.set_posix_last_error(0);
        Ok(Value::Int(result as i64))
    } else if error == 0 {
        context.set_posix_last_error(0);
        Ok(Value::Int(-1))
    } else {
        context.set_posix_last_error(error);
        Ok(Value::Bool(false))
    }
}

fn builtin_posix_times(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    no_args("posix_times", &args)?;
    let mut times = std::mem::MaybeUninit::<libc::tms>::uninit();
    let ticks = unsafe { libc::times(times.as_mut_ptr()) };
    if ticks == -1_i32 as libc::clock_t {
        context.set_posix_last_error(last_os_error());
        return Ok(Value::Bool(false));
    }
    let times = unsafe { times.assume_init() };
    let mut array = PhpArray::new();
    array.insert(string_key("ticks"), Value::Int(clock_t_to_i64(ticks)));
    array.insert(
        string_key("utime"),
        Value::Int(clock_t_to_i64(times.tms_utime)),
    );
    array.insert(
        string_key("stime"),
        Value::Int(clock_t_to_i64(times.tms_stime)),
    );
    array.insert(
        string_key("cutime"),
        Value::Int(clock_t_to_i64(times.tms_cutime)),
    );
    array.insert(
        string_key("cstime"),
        Value::Int(clock_t_to_i64(times.tms_cstime)),
    );
    context.set_posix_last_error(0);
    Ok(Value::Array(array))
}

#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    target_pointer_width = "64"
))]
fn clock_t_to_i64(value: libc::clock_t) -> i64 {
    value
}

#[cfg(not(all(
    any(target_os = "linux", target_os = "android"),
    target_pointer_width = "64"
)))]
fn clock_t_to_i64(value: libc::clock_t) -> i64 {
    value as i64
}

fn builtin_posix_get_last_error(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    no_args("posix_get_last_error", &args)?;
    Ok(Value::Int(context.posix_last_error() as i64))
}

fn builtin_posix_strerror(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("posix_strerror", "one argument"));
    }
    let errno = int_arg("posix_strerror", &args[0])? as libc::c_int;
    let message = unsafe { CStr::from_ptr(libc::strerror(errno)) }.to_string_lossy();
    Ok(Value::string(message.into_owned()))
}

fn builtin_posix_ctermid(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    no_args("posix_ctermid", &args)?;
    Ok(Value::string("/dev/tty"))
}

fn builtin_posix_getgroups(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    no_args("posix_getgroups", &args)?;
    let count = unsafe { libc::getgroups(0, std::ptr::null_mut()) };
    if count < 0 {
        context.set_posix_last_error(last_os_error());
        return Ok(Value::Bool(false));
    }
    let mut groups = vec![0 as libc::gid_t; count as usize];
    let result = unsafe { libc::getgroups(count, groups.as_mut_ptr()) };
    if result < 0 {
        context.set_posix_last_error(last_os_error());
        return Ok(Value::Bool(false));
    }
    let mut array = PhpArray::new();
    for (index, gid) in groups.into_iter().take(result as usize).enumerate() {
        array.insert(ArrayKey::Int(index as i64), Value::Int(gid as i64));
    }
    context.set_posix_last_error(0);
    Ok(Value::Array(array))
}

fn builtin_posix_getlogin(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    no_args("posix_getlogin", &args)?;
    let login = unsafe { libc::getlogin() };
    if login.is_null() {
        context.set_posix_last_error(last_os_error());
        return Ok(Value::Bool(false));
    }
    context.set_posix_last_error(0);
    Ok(Value::string(c_string(login)))
}

fn builtin_posix_getgrgid(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("posix_getgrgid", "one argument"));
    }
    let gid = int_arg("posix_getgrgid", &args[0])?;
    if gid < 0 {
        #[cfg(target_os = "macos")]
        if gid == -1 {
            let name = CString::new("nogroup").expect("static group name has no nul bytes");
            let group = unsafe { libc::getgrnam(name.as_ptr()) };
            if !group.is_null() {
                context.set_posix_last_error(0);
                return Ok(group_value_without_members(unsafe { &*group }));
            }
        }
        context.set_posix_last_error(libc::EINVAL);
        return Ok(Value::Bool(false));
    }
    let group = unsafe { libc::getgrgid(gid as libc::gid_t) };
    if group.is_null() {
        context.set_posix_last_error(last_os_error());
        return Ok(Value::Bool(false));
    }
    context.set_posix_last_error(0);
    Ok(group_value(unsafe { &*group }))
}

fn builtin_posix_getgrnam(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("posix_getgrnam", "one argument"));
    }
    let name = string_arg("posix_getgrnam", &args[0])?;
    let Ok(c_name) = CString::new(name.as_bytes()) else {
        context.set_posix_last_error(libc::EINVAL);
        return Ok(Value::Bool(false));
    };
    let group = unsafe { libc::getgrnam(c_name.as_ptr()) };
    if group.is_null() {
        context.set_posix_last_error(last_os_error());
        return Ok(Value::Bool(false));
    }
    context.set_posix_last_error(0);
    Ok(group_value(unsafe { &*group }))
}

fn builtin_posix_getpwuid(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("posix_getpwuid", "one argument"));
    }
    let uid = int_arg("posix_getpwuid", &args[0])?;
    if uid < 0 {
        context.set_posix_last_error(libc::EINVAL);
        return Ok(Value::Bool(false));
    }
    let passwd = unsafe { libc::getpwuid(uid as libc::uid_t) };
    if passwd.is_null() {
        context.set_posix_last_error(last_os_error());
        return Ok(Value::Bool(false));
    }
    context.set_posix_last_error(0);
    Ok(passwd_value(unsafe { &*passwd }))
}

fn builtin_posix_getpwnam(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("posix_getpwnam", "one argument"));
    }
    let name = string_arg("posix_getpwnam", &args[0])?;
    let Ok(c_name) = CString::new(name.as_bytes()) else {
        context.set_posix_last_error(libc::EINVAL);
        return Ok(Value::Bool(false));
    };
    let passwd = unsafe { libc::getpwnam(c_name.as_ptr()) };
    if passwd.is_null() {
        context.set_posix_last_error(last_os_error());
        return Ok(Value::Bool(false));
    }
    context.set_posix_last_error(0);
    Ok(passwd_value(unsafe { &*passwd }))
}

fn builtin_posix_getrlimit(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("posix_getrlimit", "zero or one arguments"));
    }
    if let Some(resource) = args.first() {
        let resource = int_arg("posix_getrlimit", resource)? as libc::c_int;
        let Some((soft, hard)) = get_rlimit(context, resource) else {
            return Ok(Value::Bool(false));
        };
        let mut array = PhpArray::new();
        array.insert(ArrayKey::Int(0), rlimit_value(soft));
        array.insert(ArrayKey::Int(1), rlimit_value(hard));
        return Ok(Value::Array(array));
    }

    let mut array = PhpArray::new();
    for (resource, name) in rlimit_resources() {
        let Some((soft, hard)) = get_rlimit(context, resource) else {
            return Ok(Value::Bool(false));
        };
        array.insert(string_key(&format!("soft {name}")), rlimit_value(soft));
        array.insert(string_key(&format!("hard {name}")), rlimit_value(hard));
    }
    Ok(Value::Array(array))
}

fn builtin_posix_false_gap(
    context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    context.set_posix_last_error(libc::ENOSYS);
    Ok(Value::Bool(false))
}

fn builtin_posix_permission_gap(
    context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    context.set_posix_last_error(libc::EPERM);
    Ok(Value::Bool(false))
}

fn group_value(group: &libc::group) -> Value {
    let mut array = PhpArray::new();
    array.insert(string_key("name"), Value::string(c_string(group.gr_name)));
    array.insert(
        string_key("passwd"),
        nullable_c_string_value(group.gr_passwd),
    );
    let mut members = PhpArray::new();
    if !group.gr_mem.is_null() {
        let mut index = 0usize;
        loop {
            let member = unsafe { *group.gr_mem.add(index) };
            if member.is_null() {
                break;
            }
            members.insert(ArrayKey::Int(index as i64), Value::string(c_string(member)));
            index += 1;
        }
    }
    array.insert(string_key("members"), Value::Array(members));
    array.insert(string_key("gid"), Value::Int(group.gr_gid as i64));
    Value::Array(array)
}

#[cfg(target_os = "macos")]
fn group_value_without_members(group: &libc::group) -> Value {
    let mut array = PhpArray::new();
    array.insert(string_key("name"), Value::string(c_string(group.gr_name)));
    array.insert(
        string_key("passwd"),
        nullable_c_string_value(group.gr_passwd),
    );
    array.insert(string_key("members"), Value::Array(PhpArray::new()));
    array.insert(string_key("gid"), Value::Int(group.gr_gid as i64));
    Value::Array(array)
}

fn passwd_value(passwd: &libc::passwd) -> Value {
    let mut array = PhpArray::new();
    array.insert(string_key("name"), Value::string(c_string(passwd.pw_name)));
    array.insert(
        string_key("passwd"),
        Value::string(c_string(passwd.pw_passwd)),
    );
    array.insert(string_key("uid"), Value::Int(passwd.pw_uid as i64));
    array.insert(string_key("gid"), Value::Int(passwd.pw_gid as i64));
    array.insert(
        string_key("gecos"),
        Value::string(c_string(passwd.pw_gecos)),
    );
    array.insert(string_key("dir"), Value::string(c_string(passwd.pw_dir)));
    array.insert(
        string_key("shell"),
        Value::string(c_string(passwd.pw_shell)),
    );
    Value::Array(array)
}

fn fd_arg(name: &str, value: &Value) -> Option<libc::c_int> {
    match value {
        Value::Int(fd) if *fd >= 0 && *fd <= libc::c_int::MAX as i64 => Some(*fd as libc::c_int),
        Value::Reference(reference) => fd_arg(name, &reference.get()),
        Value::Resource(_) => None,
        _ => {
            let _ = name;
            None
        }
    }
}

fn get_rlimit(context: &mut BuiltinContext<'_>, resource: libc::c_int) -> Option<(u64, u64)> {
    let mut limit = std::mem::MaybeUninit::<libc::rlimit>::uninit();
    let result = unsafe { libc::getrlimit(resource as _, limit.as_mut_ptr()) };
    if result != 0 {
        context.set_posix_last_error(last_os_error());
        return None;
    }
    context.set_posix_last_error(0);
    let limit = unsafe { limit.assume_init() };
    Some((limit.rlim_cur, limit.rlim_max))
}

fn rlimit_resources() -> Vec<(libc::c_int, &'static str)> {
    vec![
        (libc::RLIMIT_CORE as libc::c_int, "core"),
        (libc::RLIMIT_DATA as libc::c_int, "data"),
        (libc::RLIMIT_STACK as libc::c_int, "stack"),
        (libc::RLIMIT_RSS as libc::c_int, "rss"),
        (libc::RLIMIT_CPU as libc::c_int, "cpu"),
        (libc::RLIMIT_FSIZE as libc::c_int, "filesize"),
        (libc::RLIMIT_NOFILE as libc::c_int, "openfiles"),
    ]
}

fn rlimit_value(value: u64) -> Value {
    if value == libc::RLIM_INFINITY {
        Value::string("unlimited")
    } else {
        Value::Int(value.min(i64::MAX as u64) as i64)
    }
}

fn c_string(ptr: *const libc::c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }
}

fn nullable_c_string_value(ptr: *const libc::c_char) -> Value {
    if ptr.is_null() {
        Value::Null
    } else {
        Value::string(c_string(ptr))
    }
}

fn no_args(name: &str, args: &[Value]) -> Result<(), crate::builtins::BuiltinError> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(arity_error(name, "no arguments"))
    }
}

fn syscall_int_result(context: &mut BuiltinContext<'_>, result: libc::pid_t) -> BuiltinResult {
    if result >= 0 {
        context.set_posix_last_error(0);
        Ok(Value::Int(result as i64))
    } else {
        context.set_posix_last_error(last_os_error());
        Ok(Value::Bool(false))
    }
}

fn last_os_error() -> i32 {
    io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EIO)
}

fn clear_errno() {
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
    unsafe {
        *libc::__error() = 0;
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    unsafe {
        *libc::__errno_location() = 0;
    }
}

fn uts_field(field: &[libc::c_char]) -> String {
    unsafe { CStr::from_ptr(field.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn string_key(value: &str) -> ArrayKey {
    ArrayKey::String(PhpString::from_test_str(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;

    fn call_with_context(context: &mut BuiltinContext<'_>, name: &str, args: Vec<Value>) -> Value {
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .unwrap()
            .function()(context, args, RuntimeSourceSpan::default())
        .unwrap()
    }

    fn call_error_with_context(
        context: &mut BuiltinContext<'_>,
        name: &str,
        args: Vec<Value>,
    ) -> String {
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .unwrap()
            .function()(context, args, RuntimeSourceSpan::default())
        .unwrap_err()
        .message()
        .to_string()
    }

    #[test]
    fn identity_and_uname_return_host_backed_values() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        assert!(matches!(
            call_with_context(&mut context, "posix_getpid", vec![]),
            Value::Int(pid) if pid > 0
        ));
        assert!(matches!(
            call_with_context(&mut context, "posix_getuid", vec![]),
            Value::Int(uid) if uid >= 0
        ));
        let Value::Array(uname) = call_with_context(&mut context, "posix_uname", vec![]) else {
            panic!("expected uname array");
        };
        assert!(uname.get(&string_key("sysname")).is_some());
        assert!(uname.get(&string_key("machine")).is_some());
    }

    #[test]
    fn access_updates_last_error_and_strerror_uses_errno_table() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let missing = format!("/tmp/phrust-posix-missing-{}", unsafe {
            libc::getpid() as i64
        });

        assert_eq!(
            call_with_context(&mut context, "posix_access", vec![Value::string(missing)]),
            Value::Bool(false)
        );
        assert_eq!(
            call_with_context(&mut context, "posix_get_last_error", vec![]),
            Value::Int(libc::ENOENT as i64)
        );
        assert!(matches!(
            call_with_context(&mut context, "posix_strerror", vec![Value::Int(libc::ENOENT as i64)]),
            Value::String(message) if !message.as_bytes().is_empty()
        ));
    }

    #[test]
    fn passwd_group_and_limit_helpers_are_host_backed() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        let uid = unsafe { libc::getuid() as i64 };
        let gid = unsafe { libc::getgid() as i64 };
        let Value::Array(passwd) =
            call_with_context(&mut context, "posix_getpwuid", vec![Value::Int(uid)])
        else {
            panic!("expected passwd array");
        };
        assert!(passwd.get(&string_key("name")).is_some());
        let Value::Array(group) =
            call_with_context(&mut context, "posix_getgrgid", vec![Value::Int(gid)])
        else {
            panic!("expected group array");
        };
        assert!(group.get(&string_key("members")).is_some());
        assert!(matches!(
            call_with_context(&mut context, "posix_getgroups", vec![]),
            Value::Array(_)
        ));
        assert!(matches!(
            call_with_context(&mut context, "posix_getrlimit", vec![]),
            Value::Array(_)
        ));
    }

    #[test]
    fn eaccess_and_getsid_validate_upstream_value_errors() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let long_path = "bogus path".repeat(1042);

        assert_eq!(
            call_error_with_context(
                &mut context,
                "posix_eaccess",
                vec![Value::string(long_path)]
            ),
            "posix_eaccess(): Argument #1 ($filename) must not be empty"
        );

        assert_eq!(
            call_error_with_context(&mut context, "posix_getsid", vec![Value::Int(-1)]),
            format!(
                "posix_getsid(): Argument #1 ($process_id) must be between 0 and {}",
                i64::MAX
            )
        );
    }

    #[test]
    fn pathconf_and_sysconf_follow_posix_errno_disambiguation() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        assert_eq!(
            call_error_with_context(
                &mut context,
                "posix_pathconf",
                vec![Value::string(""), Value::Int(libc::_PC_PATH_MAX as i64)]
            ),
            "posix_pathconf(): Argument #1 ($path) must not be empty"
        );

        assert_eq!(
            call_with_context(&mut context, "posix_sysconf", vec![Value::Int(-1)]),
            Value::Int(-1)
        );
        assert_eq!(
            call_with_context(&mut context, "posix_errno", vec![]),
            Value::Int(0)
        );
    }

    #[test]
    fn unsupported_setuid_reports_permission_errno() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        assert_eq!(
            call_with_context(&mut context, "posix_setuid", vec![Value::Int(0)]),
            Value::Bool(false)
        );
        assert_eq!(
            call_with_context(&mut context, "posix_errno", vec![]),
            Value::Int(libc::EPERM as i64)
        );
    }
}
