//! Deterministic System V semaphore compatibility slice.

// Bounded SysV IPC via host libc calls; unsafe blocks are direct syscall
// wrappers with checked results.
#![allow(unsafe_code)]

use super::core::{argument_type_error, arity_error, int_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan, SysvSemaphoreError,
};
use crate::{ClassEntry, ClassFlags, ObjectRef, Value, normalize_class_name};

const SEMAPHORE_CLASS: &str = "SysvSemaphore";
const SEMAPHORE_ID_PROPERTY: &str = "__sysvsem_id";

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("sem_get", builtin_sem_get, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "sem_acquire",
        builtin_sem_acquire,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sem_release",
        builtin_sem_release,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("sem_remove", builtin_sem_remove, BuiltinCompatibility::Php),
];

fn builtin_sem_get(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_between("sem_get", &args, 1, 4)?;
    let key = int_arg("sem_get", &args[0])?;
    let max_acquire = optional_int("sem_get", &args, 1, 1)?;
    let permissions = optional_int("sem_get", &args, 2, 0o666)?;
    let auto_release = optional_bool("sem_get", &args, 3, true)?;
    if max_acquire <= 0 {
        return Ok(Value::Bool(false));
    }
    match context
        .sysvsem_state()
        .get(key, max_acquire, permissions, auto_release)
    {
        Ok(id) => Ok(Value::Object(semaphore_object(id))),
        Err(error) => {
            emit_sysvsem_warning(context, "sem_get", error, span);
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_sem_acquire(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_between("sem_acquire", &args, 1, 2)?;
    let semaphore_id = semaphore_id("sem_acquire", &args[0])?;
    let non_blocking = optional_bool("sem_acquire", &args, 1, false)?;
    match context.sysvsem_state().acquire(semaphore_id, non_blocking) {
        Ok(acquired) => Ok(Value::Bool(acquired)),
        Err(SysvSemaphoreError::WouldBlock) => Ok(Value::Bool(false)),
        Err(error) => {
            emit_sysvsem_warning(context, "sem_acquire", error, span);
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_sem_release(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("sem_release", &args, 1)?;
    let semaphore_id = semaphore_id("sem_release", &args[0])?;
    if context.pcntl_state().has_forked() {
        flush_root_output_to_stdout(context.output());
    }
    match context.sysvsem_state().release(semaphore_id) {
        Ok(released) => Ok(Value::Bool(released)),
        Err(error) => {
            emit_sysvsem_warning(context, "sem_release", error, span);
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_sem_remove(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("sem_remove", &args, 1)?;
    let semaphore_id = semaphore_id("sem_remove", &args[0])?;
    match context.sysvsem_state().remove(semaphore_id) {
        Ok(removed) => Ok(Value::Bool(removed)),
        Err(error) => {
            emit_sysvsem_warning(context, "sem_remove", error, span);
            Ok(Value::Bool(false))
        }
    }
}

fn emit_sysvsem_warning(
    context: &mut BuiltinContext<'_>,
    function: &str,
    error: SysvSemaphoreError,
    span: RuntimeSourceSpan,
) {
    let SysvSemaphoreError::Warning(message) = error else {
        return;
    };
    context.php_warning(
        "E_PHP_RUNTIME_SYSVSEM",
        format!("{function}(): {message}"),
        span,
    );
}

#[allow(unsafe_code)] // direct libc write loop, return value checked
fn flush_root_output_to_stdout(output: &mut crate::OutputBuffer) {
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

fn expect_exact(name: &str, args: &[Value], expected: usize) -> Result<(), BuiltinError> {
    expect_between(name, args, expected, expected)
}

fn expect_between(name: &str, args: &[Value], min: usize, max: usize) -> Result<(), BuiltinError> {
    if (min..=max).contains(&args.len()) {
        Ok(())
    } else {
        Err(arity_error(
            name,
            &format!("between {min} and {max} arguments"),
        ))
    }
}

fn optional_int(
    name: &str,
    args: &[Value],
    index: usize,
    default: i64,
) -> Result<i64, BuiltinError> {
    args.get(index)
        .map_or(Ok(default), |value| int_arg(name, value))
}

fn optional_bool(
    name: &str,
    args: &[Value],
    index: usize,
    default: bool,
) -> Result<bool, BuiltinError> {
    args.get(index).map_or(Ok(default), |value| {
        crate::convert::to_bool(value).map_err(|message| {
            BuiltinError::new("E_PHP_RUNTIME_BUILTIN_TYPE", format!("{name}(): {message}"))
        })
    })
}

fn semaphore_id(name: &str, value: &Value) -> Result<i64, BuiltinError> {
    let Value::Object(object) = value else {
        return Err(argument_type_error(
            name,
            "#1 ($semaphore)",
            SEMAPHORE_CLASS,
            value,
        ));
    };
    if normalize_class_name(&object.class_name()) != "sysvsemaphore" {
        return Err(argument_type_error(
            name,
            "#1 ($semaphore)",
            SEMAPHORE_CLASS,
            value,
        ));
    }
    match object.get_property(SEMAPHORE_ID_PROPERTY) {
        Some(Value::Int(id)) if id > 0 => Ok(id),
        _ => Err(BuiltinError::new(
            "E_PHP_RUNTIME_SYSVSEM_INVALID",
            format!("{name}(): SysvSemaphore object is no longer valid"),
        )),
    }
}

fn semaphore_object(id: i64) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&runtime_class(SEMAPHORE_CLASS), SEMAPHORE_CLASS);
    object.set_property(SEMAPHORE_ID_PROPERTY, Value::Int(id));
    object
}

fn runtime_class(name: &str) -> ClassEntry {
    ClassEntry {
        name: normalize_class_name(name),
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

    /// Derives a per-process test key and removes any semaphore set a
    /// crashed previous run left behind under it (the 16-bit pid slice
    /// wraps).
    fn fresh_sysvsem_key() -> i64 {
        let key = 0x7072_0000_i64 | (i64::from(std::process::id()) & 0xffff);
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        if let Ok(semaphore) = builtin_sem_get(
            &mut context,
            vec![Value::Int(key)],
            RuntimeSourceSpan::default(),
        ) {
            let _ = builtin_sem_remove(&mut context, vec![semaphore], RuntimeSourceSpan::default());
        }
        key
    }

    #[test]
    fn semaphore_tracks_acquire_release_and_remove() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let key = fresh_sysvsem_key();
        let semaphore = builtin_sem_get(
            &mut context,
            vec![Value::Int(key), Value::Int(1), Value::Int(0o600)],
            RuntimeSourceSpan::default(),
        )
        .expect("semaphore");

        assert_eq!(
            builtin_sem_acquire(
                &mut context,
                vec![semaphore.clone()],
                RuntimeSourceSpan::default(),
            )
            .expect("first acquire"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_sem_acquire(
                &mut context,
                vec![semaphore.clone(), Value::Bool(true)],
                RuntimeSourceSpan::default(),
            )
            .expect("second acquire"),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_sem_release(
                &mut context,
                vec![semaphore.clone()],
                RuntimeSourceSpan::default(),
            )
            .expect("release"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_sem_remove(&mut context, vec![semaphore], RuntimeSourceSpan::default(),)
                .expect("remove"),
            Value::Bool(true)
        );
    }
}
