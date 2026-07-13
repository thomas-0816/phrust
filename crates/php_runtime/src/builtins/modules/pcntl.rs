//! CLI-only PCNTL compatibility slice backed by Unix libc calls.

#![allow(unsafe_code)]

use super::core::{
    argument_type_error, argument_value_error, arity_error, assign_reference_arg, deref_value,
    int_arg, string_arg,
};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ArrayKey, PhpArray, Value, to_bool};
use std::ffi::{CStr, CString};
use std::io;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "pcntl_alarm",
        builtin_pcntl_alarm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pcntl_async_signals",
        builtin_pcntl_async_signals,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pcntl_errno",
        builtin_pcntl_errno,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("pcntl_exec", builtin_pcntl_exec, BuiltinCompatibility::Php),
    BuiltinEntry::new("pcntl_fork", builtin_pcntl_fork, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "pcntl_get_last_error",
        builtin_pcntl_errno,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pcntl_getpriority",
        builtin_pcntl_getpriority,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pcntl_setpriority",
        builtin_pcntl_setpriority,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pcntl_signal",
        builtin_pcntl_signal,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pcntl_signal_dispatch",
        builtin_pcntl_signal_dispatch,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pcntl_signal_get_handler",
        builtin_pcntl_signal_get_handler,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pcntl_strerror",
        builtin_pcntl_strerror,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("pcntl_wait", builtin_pcntl_wait, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "pcntl_waitpid",
        builtin_pcntl_waitpid,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pcntl_wexitstatus",
        builtin_pcntl_wexitstatus,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pcntl_wifcontinued",
        builtin_pcntl_wifcontinued,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pcntl_wifexited",
        builtin_pcntl_wifexited,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pcntl_wifsignaled",
        builtin_pcntl_wifsignaled,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pcntl_wifstopped",
        builtin_pcntl_wifstopped,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pcntl_wstopsig",
        builtin_pcntl_wstopsig,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pcntl_wtermsig",
        builtin_pcntl_wtermsig,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_pcntl_alarm(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("pcntl_alarm", &args, 1)?;
    let seconds = int_arg("pcntl_alarm", &args[0])?;
    let previous = unsafe { libc::alarm(seconds.max(0) as libc::c_uint) };
    context.pcntl_state().set_last_error(0);
    Ok(Value::Int(previous as i64))
}

fn builtin_pcntl_async_signals(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_between("pcntl_async_signals", &args, 0, 1)?;
    let current = context.pcntl_state().async_signals();
    if args.is_empty() || matches!(args[0], Value::Null) {
        return Ok(Value::Bool(current));
    }
    let enabled = to_bool(&args[0])
        .map_err(|message| super::core::conversion_error("pcntl_async_signals", message))?;
    let previous = context.pcntl_state().set_async_signals(enabled);
    Ok(Value::Bool(previous))
}

fn builtin_pcntl_errno(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("pcntl_get_last_error", &args, 0)?;
    Ok(Value::Int(context.pcntl_state().last_error() as i64))
}

fn builtin_pcntl_exec(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_between("pcntl_exec", &args, 1, 3)?;
    let path = string_arg("pcntl_exec", &args[0])?.to_string_lossy();
    let c_path = match CString::new(path.as_bytes()) {
        Ok(path) => path,
        Err(_) => {
            context.pcntl_state().set_last_error(libc::EINVAL);
            return Ok(Value::Bool(false));
        }
    };
    let argv = exec_argv(&args, path.as_ref())?;
    let envp = exec_env(&args)?;
    let mut argv_ptrs = argv.iter().map(|arg| arg.as_ptr()).collect::<Vec<_>>();
    argv_ptrs.push(std::ptr::null());
    let mut env_ptrs = envp.iter().map(|arg| arg.as_ptr()).collect::<Vec<_>>();
    env_ptrs.push(std::ptr::null());

    unsafe {
        libc::execve(c_path.as_ptr(), argv_ptrs.as_ptr(), env_ptrs.as_ptr());
    }
    let error = last_errno();
    context.pcntl_state().set_last_error(error);
    Ok(Value::Bool(false))
}

fn builtin_pcntl_fork(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("pcntl_fork", &args, 0)?;
    flush_root_output_before_fork(context.output());
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        context.pcntl_state().set_last_error(last_errno());
    } else {
        let state = context.pcntl_state();
        state.set_last_error(0);
        state.set_fork_child(pid == 0);
    }
    Ok(Value::Int(pid as i64))
}

fn flush_root_output_before_fork(output: &mut crate::OutputBuffer) {
    if output.as_bytes().is_empty() {
        return;
    }
    let mut written = 0;
    let bytes = output.as_bytes();
    while written < bytes.len() {
        let result = unsafe {
            libc::write(
                libc::STDOUT_FILENO,
                bytes[written..].as_ptr().cast(),
                bytes.len() - written,
            )
        };
        if result <= 0 {
            break;
        }
        written += result as usize;
    }
    output.clear();
}

fn builtin_pcntl_getpriority(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_between("pcntl_getpriority", &args, 0, 2)?;
    let process_id = optional_int("pcntl_getpriority", &args, 0, 0)?;
    let mode = optional_int("pcntl_getpriority", &args, 1, libc::PRIO_PROCESS as i64)?;
    validate_getpriority_mode(process_id, mode)?;
    clear_errno();
    let priority = unsafe { libc::getpriority(mode as _, process_id as libc::id_t) };
    let error = last_errno();
    if priority == -1 && error != 0 {
        context.pcntl_state().set_last_error(error);
        return Ok(Value::Bool(false));
    }
    context.pcntl_state().set_last_error(0);
    Ok(Value::Int(priority as i64))
}

fn builtin_pcntl_setpriority(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_between("pcntl_setpriority", &args, 1, 3)?;
    let priority = int_arg("pcntl_setpriority", &args[0])?;
    let process_id = optional_int("pcntl_setpriority", &args, 1, 0)?;
    let mode = optional_int("pcntl_setpriority", &args, 2, libc::PRIO_PROCESS as i64)?;
    validate_setpriority_mode(priority, process_id, mode)?;
    let result =
        unsafe { libc::setpriority(mode as _, process_id as libc::id_t, priority as libc::c_int) };
    if result == 0 {
        context.pcntl_state().set_last_error(0);
        Ok(Value::Bool(true))
    } else {
        let error = last_errno();
        context.pcntl_state().set_last_error(error);
        match error {
            libc::ESRCH => context.php_warning(
                "E_PHP_RUNTIME_PCNTL_SETPRIORITY",
                format!("pcntl_setpriority(): Error {error}: No process was located using the given parameters"),
                span,
            ),
            libc::EPERM => context.php_warning(
                "E_PHP_RUNTIME_PCNTL_SETPRIORITY",
                format!(
                    "pcntl_setpriority(): Error {error}: A process was located, but neither its effective nor real user ID matched the effective user ID of the caller"
                ),
                span,
            ),
            libc::EACCES => context.php_warning(
                "E_PHP_RUNTIME_PCNTL_SETPRIORITY",
                format!("pcntl_setpriority(): Error {error}: Only a super user may attempt to increase the process priority"),
                span,
            ),
            _ => context.php_warning(
                "E_PHP_RUNTIME_PCNTL_SETPRIORITY",
                format!("pcntl_setpriority(): Unknown error {error} has occurred"),
                span,
            ),
        }
        Ok(Value::Bool(false))
    }
}

fn builtin_pcntl_signal(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_between("pcntl_signal", &args, 2, 3)?;
    let signal = int_arg("pcntl_signal", &args[0])?;
    if !valid_signal(signal) {
        context.pcntl_state().set_last_error(libc::EINVAL);
        return Ok(Value::Bool(false));
    }
    validate_signal_handler("pcntl_signal", &args[1])?;
    context
        .pcntl_state()
        .set_signal_handler(signal, args[1].clone());
    context.pcntl_state().set_last_error(0);
    Ok(Value::Bool(true))
}

fn builtin_pcntl_signal_dispatch(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("pcntl_signal_dispatch", &args, 0)?;
    context.pcntl_state().set_last_error(0);
    Ok(Value::Bool(true))
}

fn builtin_pcntl_signal_get_handler(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("pcntl_signal_get_handler", &args, 1)?;
    let signal = int_arg("pcntl_signal_get_handler", &args[0])?;
    if !valid_signal(signal) {
        context.pcntl_state().set_last_error(libc::EINVAL);
        return Ok(Value::Bool(false));
    }
    Ok(context
        .pcntl_state()
        .signal_handler(signal)
        .unwrap_or(Value::Int(libc::SIG_DFL as i64)))
}

fn builtin_pcntl_strerror(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("pcntl_strerror", &args, 1)?;
    let error = int_arg("pcntl_strerror", &args[0])?;
    let message = unsafe {
        let ptr = libc::strerror(error as libc::c_int);
        if ptr.is_null() {
            "Unknown error".to_owned()
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    };
    Ok(Value::string(message))
}

fn builtin_pcntl_wait(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_between("pcntl_wait", &args, 1, 3)?;
    waitpid_impl(context, -1, args.first(), args.get(1), args.get(2))
}

fn builtin_pcntl_waitpid(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_between("pcntl_waitpid", &args, 2, 4)?;
    let pid = int_arg("pcntl_waitpid", &args[0])?;
    waitpid_impl(context, pid, args.get(1), args.get(2), args.get(3))
}

fn waitpid_impl(
    context: &mut BuiltinContext<'_>,
    pid: i64,
    status_arg: Option<&Value>,
    flags_arg: Option<&Value>,
    rusage_arg: Option<&Value>,
) -> BuiltinResult {
    let flags = flags_arg
        .map(|value| int_arg("pcntl_waitpid", value))
        .transpose()?
        .unwrap_or(0);
    let mut status = 0_i32;
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    let waited = unsafe {
        libc::wait4(
            pid as libc::pid_t,
            &mut status,
            flags as libc::c_int,
            usage.as_mut_ptr(),
        )
    };
    if waited < 0 {
        context.pcntl_state().set_last_error(last_errno());
        assign_reference_arg(status_arg, Value::Int(0));
        assign_reference_arg(rusage_arg, Value::Array(PhpArray::new()));
        return Ok(Value::Int(-1));
    }
    context.pcntl_state().set_last_error(0);
    assign_reference_arg(status_arg, Value::Int(status as i64));
    let usage = unsafe { usage.assume_init() };
    assign_reference_arg(rusage_arg, rusage_value(&usage));
    Ok(Value::Int(waited as i64))
}

fn builtin_pcntl_wexitstatus(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("pcntl_wexitstatus", &args, 1)?;
    let status = int_arg("pcntl_wexitstatus", &args[0])? as libc::c_int;
    if libc::WIFEXITED(status) {
        Ok(Value::Int(libc::WEXITSTATUS(status) as i64))
    } else {
        Ok(Value::Bool(false))
    }
}

fn builtin_pcntl_wifcontinued(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("pcntl_wifcontinued", &args, 1)?;
    let status = int_arg("pcntl_wifcontinued", &args[0])? as libc::c_int;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        Ok(Value::Bool(libc::WIFCONTINUED(status)))
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        let _ = status;
        Ok(Value::Bool(false))
    }
}

fn builtin_pcntl_wifexited(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("pcntl_wifexited", &args, 1)?;
    let status = int_arg("pcntl_wifexited", &args[0])? as libc::c_int;
    Ok(Value::Bool(libc::WIFEXITED(status)))
}

fn builtin_pcntl_wifsignaled(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("pcntl_wifsignaled", &args, 1)?;
    let status = int_arg("pcntl_wifsignaled", &args[0])? as libc::c_int;
    Ok(Value::Bool(libc::WIFSIGNALED(status)))
}

fn builtin_pcntl_wifstopped(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("pcntl_wifstopped", &args, 1)?;
    let status = int_arg("pcntl_wifstopped", &args[0])? as libc::c_int;
    Ok(Value::Bool(libc::WIFSTOPPED(status)))
}

fn builtin_pcntl_wstopsig(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("pcntl_wstopsig", &args, 1)?;
    let status = int_arg("pcntl_wstopsig", &args[0])? as libc::c_int;
    if libc::WIFSTOPPED(status) {
        Ok(Value::Int(libc::WSTOPSIG(status) as i64))
    } else {
        Ok(Value::Bool(false))
    }
}

fn builtin_pcntl_wtermsig(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("pcntl_wtermsig", &args, 1)?;
    let status = int_arg("pcntl_wtermsig", &args[0])? as libc::c_int;
    if libc::WIFSIGNALED(status) {
        Ok(Value::Int(libc::WTERMSIG(status) as i64))
    } else {
        Ok(Value::Bool(false))
    }
}

fn validate_signal_handler(name: &str, value: &Value) -> Result<(), crate::builtins::BuiltinError> {
    match value {
        Value::Int(handler)
            if *handler == libc::SIG_DFL as i64 || *handler == libc::SIG_IGN as i64 =>
        {
            Ok(())
        }
        Value::String(_) | Value::Array(_) | Value::Callable(_) => Ok(()),
        Value::Reference(reference) => validate_signal_handler(name, &reference.get()),
        other => Err(argument_type_error(
            name,
            "#2 ($handler)",
            "callable|int",
            other,
        )),
    }
}

fn exec_argv(args: &[Value], argv0: &str) -> Result<Vec<CString>, BuiltinError> {
    let mut values = vec![CString::new(argv0).map_err(|_| {
        argument_value_error("pcntl_exec", "#1 ($path)", "must not contain null bytes")
    })?];
    let Some(value) = args.get(1) else {
        return Ok(values);
    };
    let Value::Array(array) = value else {
        return Err(argument_type_error(
            "pcntl_exec",
            "#2 ($args)",
            "array",
            value,
        ));
    };
    for (_, value) in array.iter() {
        let string = exec_string(value)?;
        values.push(CString::new(string.as_bytes()).map_err(|_| {
            argument_value_error(
                "pcntl_exec",
                "#2 ($args)",
                "individual argument must not contain null bytes",
            )
        })?);
    }
    Ok(values)
}

fn exec_env(args: &[Value]) -> Result<Vec<CString>, BuiltinError> {
    let mut values = Vec::new();
    let Some(value) = args.get(2) else {
        return Ok(values);
    };
    let Value::Array(array) = value else {
        return Err(argument_type_error(
            "pcntl_exec",
            "#3 ($env_vars)",
            "array",
            value,
        ));
    };
    for (key, value) in array.iter() {
        let name = match key {
            ArrayKey::String(key) => key.to_string_lossy(),
            ArrayKey::Int(key) => key.to_string(),
        };
        let value = exec_string(value)?;
        let name = CString::new(name.as_bytes()).map_err(|_| {
            argument_value_error(
                "pcntl_exec",
                "#3 ($env_vars)",
                "name for environment variable must not contain null bytes",
            )
        })?;
        let value = CString::new(value.as_bytes()).map_err(|_| {
            argument_value_error(
                "pcntl_exec",
                "#3 ($env_vars)",
                "value for environment variable must not contain null bytes",
            )
        })?;
        let mut pair = Vec::with_capacity(name.as_bytes().len() + value.as_bytes().len() + 1);
        pair.extend_from_slice(name.as_bytes());
        pair.push(b'=');
        pair.extend_from_slice(value.as_bytes());
        values.push(CString::new(pair).map_err(|_| {
            argument_value_error(
                "pcntl_exec",
                "#3 ($env_vars)",
                "environment variable pair must not contain null bytes",
            )
        })?);
    }
    Ok(values)
}

fn exec_string(value: &Value) -> Result<String, BuiltinError> {
    match deref_value(value) {
        Value::Object(object) => Err(BuiltinError::new(
            "E_PHP_VM_SPL_ERROR",
            format!(
                "Object of class {} could not be converted to string",
                object.display_name()
            ),
        )),
        value => Ok(string_arg("pcntl_exec", &value)?.to_string_lossy()),
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
const PRIO_DARWIN_BG_VALUE: i64 = 0x1000;
#[cfg(any(target_os = "macos", target_os = "ios"))]
const PRIO_DARWIN_THREAD_VALUE: i64 = 3;

fn validate_getpriority_mode(process_id: i64, mode: i64) -> Result<(), BuiltinError> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        let allowed = [
            libc::PRIO_PGRP as i64,
            libc::PRIO_USER as i64,
            libc::PRIO_PROCESS as i64,
            PRIO_DARWIN_THREAD_VALUE,
        ];
        if !allowed.contains(&mode) {
            return Err(argument_value_error(
                "pcntl_getpriority",
                "#2 ($mode)",
                "must be one of PRIO_PGRP, PRIO_USER, PRIO_PROCESS or PRIO_DARWIN_THREAD",
            ));
        }
        if mode == PRIO_DARWIN_THREAD_VALUE && process_id != 0 {
            return Err(argument_value_error(
                "pcntl_getpriority",
                "#1 ($process_id)",
                "must be 0 (zero) if PRIO_DARWIN_THREAD is provided as second parameter",
            ));
        }
        if mode == libc::PRIO_PROCESS as i64 && process_id < 0 {
            return Err(argument_value_error(
                "pcntl_getpriority",
                "#1 ($process_id)",
                "is not a valid process, process group, or user ID",
            ));
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        let allowed = [
            libc::PRIO_PGRP as i64,
            libc::PRIO_USER as i64,
            libc::PRIO_PROCESS as i64,
        ];
        if !allowed.contains(&mode) {
            return Err(argument_value_error(
                "pcntl_getpriority",
                "#2 ($mode)",
                "must be one of PRIO_PGRP, PRIO_USER, or PRIO_PROCESS",
            ));
        }
    }
    Ok(())
}

fn validate_setpriority_mode(
    priority: i64,
    process_id: i64,
    mode: i64,
) -> Result<(), BuiltinError> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        let allowed = [
            libc::PRIO_PGRP as i64,
            libc::PRIO_USER as i64,
            libc::PRIO_PROCESS as i64,
            PRIO_DARWIN_THREAD_VALUE,
        ];
        if !allowed.contains(&mode) {
            return Err(argument_value_error(
                "pcntl_setpriority",
                "#3 ($mode)",
                "must be one of PRIO_PGRP, PRIO_USER, PRIO_PROCESS or PRIO_DARWIN_THREAD",
            ));
        }
        if mode == PRIO_DARWIN_THREAD_VALUE && process_id != 0 {
            return Err(argument_value_error(
                "pcntl_setpriority",
                "#2 ($process_id)",
                "must be 0 (zero) if PRIO_DARWIN_THREAD is provided as second parameter",
            ));
        }
        if mode == PRIO_DARWIN_THREAD_VALUE && priority != 0 && priority != PRIO_DARWIN_BG_VALUE {
            return Err(argument_value_error(
                "pcntl_setpriority",
                "#1 ($priority)",
                "must be either 0 (zero) or PRIO_DARWIN_BG, for mode PRIO_DARWIN_THREAD",
            ));
        }
        if mode == libc::PRIO_PROCESS as i64 && process_id < 0 {
            return Err(argument_value_error(
                "pcntl_setpriority",
                "#2 ($process_id)",
                "is not a valid process, process group, or user ID",
            ));
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        let _ = process_id;
        let _ = priority;
        let allowed = [
            libc::PRIO_PGRP as i64,
            libc::PRIO_USER as i64,
            libc::PRIO_PROCESS as i64,
        ];
        if !allowed.contains(&mode) {
            return Err(argument_value_error(
                "pcntl_setpriority",
                "#3 ($mode)",
                "must be one of PRIO_PGRP, PRIO_USER, or PRIO_PROCESS",
            ));
        }
    }
    Ok(())
}

fn rusage_value(usage: &libc::rusage) -> Value {
    let mut array = PhpArray::new();
    array.insert(
        string_key("ru_utime.tv_sec"),
        Value::Int(usage.ru_utime.tv_sec),
    );
    array.insert(
        string_key("ru_utime.tv_usec"),
        Value::Int(suseconds_to_i64(usage.ru_utime.tv_usec)),
    );
    array.insert(
        string_key("ru_stime.tv_sec"),
        Value::Int(usage.ru_stime.tv_sec),
    );
    array.insert(
        string_key("ru_stime.tv_usec"),
        Value::Int(suseconds_to_i64(usage.ru_stime.tv_usec)),
    );
    Value::Array(array)
}

#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    target_pointer_width = "64"
))]
fn suseconds_to_i64(value: libc::suseconds_t) -> i64 {
    value
}

#[cfg(not(all(
    any(target_os = "linux", target_os = "android"),
    target_pointer_width = "64"
)))]
fn suseconds_to_i64(value: libc::suseconds_t) -> i64 {
    value as i64
}

fn valid_signal(signal: i64) -> bool {
    signal > 0 && signal < 128
}

fn expect_exact(
    name: &'static str,
    args: &[Value],
    count: usize,
) -> Result<(), crate::builtins::BuiltinError> {
    if args.len() == count {
        Ok(())
    } else {
        Err(arity_error(name, &format!("exactly {count} argument(s)")))
    }
}

fn expect_between(
    name: &'static str,
    args: &[Value],
    min: usize,
    max: usize,
) -> Result<(), crate::builtins::BuiltinError> {
    if (min..=max).contains(&args.len()) {
        Ok(())
    } else {
        Err(arity_error(
            name,
            &format!("between {min} and {max} argument(s)"),
        ))
    }
}

fn optional_int(
    name: &str,
    args: &[Value],
    index: usize,
    default: i64,
) -> Result<i64, crate::builtins::BuiltinError> {
    match args.get(index) {
        Some(Value::Null) | None => Ok(default),
        Some(value) => int_arg(name, value),
    }
}

fn string_key(key: &str) -> ArrayKey {
    ArrayKey::String(key.as_bytes().to_vec().into())
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

fn last_errno() -> i32 {
    io::Error::last_os_error().raw_os_error().unwrap_or(0)
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

    #[test]
    fn async_signals_and_signal_handlers_are_request_local() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        assert_eq!(
            call_with_context(&mut context, "pcntl_async_signals", vec![]),
            Value::Bool(false)
        );
        assert_eq!(
            call_with_context(&mut context, "pcntl_async_signals", vec![Value::Bool(true)]),
            Value::Bool(false)
        );
        assert_eq!(
            call_with_context(&mut context, "pcntl_async_signals", vec![]),
            Value::Bool(true)
        );

        let signal = libc::SIGUSR1 as i64;
        assert_eq!(
            call_with_context(
                &mut context,
                "pcntl_signal_get_handler",
                vec![Value::Int(signal)]
            ),
            Value::Int(libc::SIG_DFL as i64)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "pcntl_signal",
                vec![Value::Int(signal), Value::Int(libc::SIG_IGN as i64)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "pcntl_signal_get_handler",
                vec![Value::Int(signal)]
            ),
            Value::Int(libc::SIG_IGN as i64)
        );
        assert_eq!(
            call_with_context(
                &mut context,
                "pcntl_signal",
                vec![Value::Int(signal), Value::string("strlen")]
            ),
            Value::Bool(true)
        );
        assert!(matches!(
            call_with_context(
                &mut context,
                "pcntl_signal_get_handler",
                vec![Value::Int(signal)]
            ),
            Value::String(handler) if handler.as_bytes() == b"strlen"
        ));
    }

    #[test]
    fn wait_status_helpers_decode_exited_status_without_forking() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let status = 23_i64 << 8;

        assert_eq!(
            call_with_context(&mut context, "pcntl_wifexited", vec![Value::Int(status)]),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_context(&mut context, "pcntl_wexitstatus", vec![Value::Int(status)]),
            Value::Int(23)
        );
        assert_eq!(
            call_with_context(&mut context, "pcntl_wifsignaled", vec![Value::Int(status)]),
            Value::Bool(false)
        );
        assert_eq!(
            call_with_context(&mut context, "pcntl_wtermsig", vec![Value::Int(status)]),
            Value::Bool(false)
        );
    }

    #[test]
    fn strerror_and_errno_surface_uses_pcntl_state() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        assert_eq!(
            call_with_context(&mut context, "pcntl_errno", vec![]),
            Value::Int(0)
        );
        assert!(matches!(
            call_with_context(&mut context, "pcntl_strerror", vec![Value::Int(libc::ENOENT as i64)]),
            Value::String(message) if !message.as_bytes().is_empty()
        ));
        assert_eq!(
            call_with_context(
                &mut context,
                "pcntl_signal",
                vec![Value::Int(999), Value::Int(libc::SIG_DFL as i64)]
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call_with_context(&mut context, "pcntl_get_last_error", vec![]),
            Value::Int(libc::EINVAL as i64)
        );
    }
}
