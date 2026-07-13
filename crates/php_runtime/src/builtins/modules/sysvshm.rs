//! System V shared variable compatibility slice.

use super::core::{argument_type_error, argument_value_error, arity_error, int_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ClassEntry, ClassFlags, ObjectRef, Value, normalize_class_name};

const SHM_CLASS: &str = "SysvSharedMemory";

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("shm_attach", builtin_shm_attach, BuiltinCompatibility::Php),
    BuiltinEntry::new("shm_detach", builtin_shm_detach, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "shm_has_var",
        builtin_shm_has_var,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "shm_put_var",
        builtin_shm_put_var,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "shm_get_var",
        builtin_shm_get_var,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "shm_remove_var",
        builtin_shm_remove_var,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("shm_remove", builtin_shm_remove, BuiltinCompatibility::Php),
];

fn builtin_shm_attach(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_between("shm_attach", &args, 1, 3)?;
    let key = int_arg("shm_attach", &args[0])?;
    let size = args
        .get(1)
        .filter(|value| !matches!(value, Value::Null))
        .map_or(Ok(10_000), |value| int_arg("shm_attach", value))?;
    if size <= 0 {
        return Err(argument_value_error(
            "shm_attach",
            "#2 ($size)",
            "must be greater than 0",
        ));
    }
    let permissions = optional_int("shm_attach", &args, 2, 0o666)?;
    let id = match context.sysvshm_state().attach(key, size, permissions) {
        Ok(id) => id,
        Err(errno) => {
            context.php_warning(
                "E_PHP_RUNTIME_SYSVSHM_ATTACH",
                format!(
                    "shm_attach(): failed for key 0x{key:x}: {}",
                    std::io::Error::from_raw_os_error(errno)
                ),
                _span,
            );
            return Ok(Value::Bool(false));
        }
    };
    let object = shm_object();
    context.sysvshm_state().bind_object(object.id(), id);
    Ok(Value::Object(object))
}

fn builtin_shm_detach(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("shm_detach", &args, 1)?;
    let object_id = shm_bound_object_id(context, "shm_detach", &args[0])?;
    context.sysvshm_state().destroy_object(object_id);
    Ok(Value::Bool(true))
}

fn builtin_shm_has_var(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("shm_has_var", &args, 2)?;
    let shm_id = shm_handle(context, "shm_has_var", &args[0])?.segment_id;
    let key = int_arg("shm_has_var", &args[1])?;
    Ok(Value::Bool(
        context
            .sysvshm_state()
            .segment(shm_id)
            .is_some_and(|segment| segment.has(key)),
    ))
}

fn builtin_shm_put_var(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("shm_put_var", &args, 3)?;
    let shm_id = shm_handle(context, "shm_put_var", &args[0])?.segment_id;
    let key = int_arg("shm_put_var", &args[1])?;
    let serialized = serialized_value(&args[2])?;
    let stored_size = serialized.len();
    let Some(segment) = context.sysvshm_state().segment_mut(shm_id) else {
        return Ok(Value::Bool(false));
    };
    if !segment.can_store(key, stored_size) {
        context.php_warning(
            "E_PHP_RUNTIME_SYSVSHM_NO_SPACE",
            "shm_put_var(): Not enough shared memory left",
            _span,
        );
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(segment.put_serialized(key, serialized)))
}

fn builtin_shm_get_var(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("shm_get_var", &args, 2)?;
    let shm_id = shm_handle(context, "shm_get_var", &args[0])?.segment_id;
    let key = int_arg("shm_get_var", &args[1])?;
    let Some(segment) = context.sysvshm_state().segment(shm_id) else {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_SYSVSHM_INVALID",
            "shm_get_var(): SysvSharedMemory object is no longer valid",
        ));
    };
    if let Some(value) = segment.get(key) {
        Ok(value)
    } else {
        context.php_warning(
            "E_PHP_RUNTIME_SYSVSHM_KEY",
            format!("shm_get_var(): Variable key {key} doesn't exist"),
            _span,
        );
        Ok(Value::Bool(false))
    }
}

fn builtin_shm_remove_var(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("shm_remove_var", &args, 2)?;
    let shm_id = shm_handle(context, "shm_remove_var", &args[0])?.segment_id;
    let key = int_arg("shm_remove_var", &args[1])?;
    let Some(segment) = context.sysvshm_state().segment_mut(shm_id) else {
        return Ok(Value::Bool(false));
    };
    if segment.remove_var(key) {
        Ok(Value::Bool(true))
    } else {
        context.php_warning(
            "E_PHP_RUNTIME_SYSVSHM_KEY",
            format!("shm_remove_var(): Variable key {key} doesn't exist"),
            _span,
        );
        Ok(Value::Bool(false))
    }
}

fn builtin_shm_remove(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("shm_remove", &args, 1)?;
    let handle = shm_handle(context, "shm_remove", &args[0])?;
    let removed = context.sysvshm_state().remove(handle.segment_id);
    Ok(Value::Bool(removed))
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ShmHandle {
    segment_id: i64,
}

fn shm_bound_object_id(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
) -> Result<u64, BuiltinError> {
    let Value::Object(object) = value else {
        return Err(argument_type_error(name, "#1 ($shm)", SHM_CLASS, value));
    };
    if normalize_class_name(&object.class_name()) != "sysvsharedmemory" {
        return Err(argument_type_error(name, "#1 ($shm)", SHM_CLASS, value));
    }
    if context.sysvshm_state().object_destroyed(object.id()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_SYSVSHM_INVALID",
            "Shared memory block has already been destroyed",
        ));
    }
    context
        .sysvshm_state()
        .bound_segment_id_for_object(object.id())
        .map(|_| object.id())
        .ok_or_else(|| {
            BuiltinError::new(
                "E_PHP_RUNTIME_SYSVSHM_INVALID",
                "Shared memory block has already been destroyed",
            )
        })
}

fn shm_handle(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
) -> Result<ShmHandle, BuiltinError> {
    let Value::Object(object) = value else {
        return Err(argument_type_error(name, "#1 ($shm)", SHM_CLASS, value));
    };
    if normalize_class_name(&object.class_name()) != "sysvsharedmemory" {
        return Err(argument_type_error(name, "#1 ($shm)", SHM_CLASS, value));
    }
    if context.sysvshm_state().object_destroyed(object.id()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_SYSVSHM_INVALID",
            "Shared memory block has already been destroyed",
        ));
    }
    context
        .sysvshm_state()
        .segment_id_for_object(object.id())
        .map(|segment_id| ShmHandle { segment_id })
        .ok_or_else(|| {
            BuiltinError::new(
                "E_PHP_RUNTIME_SYSVSHM_INVALID",
                format!("{name}(): SysvSharedMemory object is no longer valid"),
            )
        })
}

fn shm_object() -> ObjectRef {
    ObjectRef::new_with_display_name(&runtime_class(SHM_CLASS), SHM_CLASS)
}

fn serialized_value(value: &Value) -> Result<Vec<u8>, BuiltinError> {
    crate::serialize(value)
        .map(|serialized| serialized.as_bytes().to_vec())
        .map_err(|error| BuiltinError::new("E_PHP_RUNTIME_SYSVSHM_SERIALIZE", error.message()))
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

    /// Reuses a bounded test-key namespace and removes any segment a crashed
    /// previous run left behind. Process-derived keys leak one segment per
    /// killed run and can exhaust the host's global shared-memory limit.
    fn unique_sysvshm_key(offset: i64) -> i64 {
        let key = 0x54ff_ff00_i64 | (offset & 0xff);
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        if let Ok(shm) = builtin_shm_attach(
            &mut context,
            vec![Value::Int(key)],
            RuntimeSourceSpan::default(),
        ) {
            let _ = builtin_shm_remove(&mut context, vec![shm], RuntimeSourceSpan::default());
        }
        key
    }

    #[test]
    fn shared_memory_stores_variables_by_key_and_removes_segment() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let shm = builtin_shm_attach(
            &mut context,
            vec![
                Value::Int(unique_sysvshm_key(1)),
                Value::Int(1024),
                Value::Int(0o600),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("attach");

        assert_eq!(
            builtin_shm_put_var(
                &mut context,
                vec![shm.clone(), Value::Int(1), Value::string("value")],
                RuntimeSourceSpan::default(),
            )
            .expect("put"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_shm_has_var(
                &mut context,
                vec![shm.clone(), Value::Int(1)],
                RuntimeSourceSpan::default(),
            )
            .expect("has"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_shm_get_var(
                &mut context,
                vec![shm.clone(), Value::Int(1)],
                RuntimeSourceSpan::default(),
            )
            .expect("get"),
            Value::string("value")
        );
        assert_eq!(
            builtin_shm_remove(&mut context, vec![shm], RuntimeSourceSpan::default(),)
                .expect("remove"),
            Value::Bool(true)
        );
    }

    #[test]
    fn shared_memory_object_does_not_expose_internal_id_property() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let shm = builtin_shm_attach(
            &mut context,
            vec![Value::Int(unique_sysvshm_key(2)), Value::Int(1024)],
            RuntimeSourceSpan::default(),
        )
        .expect("attach");
        let Value::Object(object) = &shm else {
            panic!("expected shared-memory object");
        };

        assert_eq!(object.get_property("__sysvshm_id"), None);
        let _ = builtin_shm_remove(&mut context, vec![shm], RuntimeSourceSpan::default());
    }

    #[test]
    fn missing_variable_operations_warn_and_return_false() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let shm = builtin_shm_attach(
            &mut context,
            vec![Value::Int(unique_sysvshm_key(3)), Value::Int(1024)],
            RuntimeSourceSpan::default(),
        )
        .expect("attach");

        assert_eq!(
            builtin_shm_get_var(
                &mut context,
                vec![shm.clone(), Value::Int(99)],
                RuntimeSourceSpan::default(),
            )
            .expect("get"),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_shm_remove_var(
                &mut context,
                vec![shm.clone(), Value::Int(99)],
                RuntimeSourceSpan::default(),
            )
            .expect("remove var"),
            Value::Bool(false)
        );
        let _ = builtin_shm_remove(&mut context, vec![shm], RuntimeSourceSpan::default());
        let output = output.to_string_lossy();
        assert!(output.contains("shm_get_var(): Variable key 99 doesn't exist"));
        assert!(output.contains("shm_remove_var(): Variable key 99 doesn't exist"));
    }

    #[test]
    fn put_var_respects_segment_size_budget() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let shm = builtin_shm_attach(
            &mut context,
            vec![Value::Int(unique_sysvshm_key(4)), Value::Int(16)],
            RuntimeSourceSpan::default(),
        )
        .expect("attach");

        assert_eq!(
            builtin_shm_put_var(
                &mut context,
                vec![
                    shm.clone(),
                    Value::Int(1),
                    Value::string("this value is too large"),
                ],
                RuntimeSourceSpan::default(),
            )
            .expect("put"),
            Value::Bool(false)
        );
        let _ = builtin_shm_remove(&mut context, vec![shm], RuntimeSourceSpan::default());
        assert!(
            output
                .to_string_lossy()
                .contains("shm_put_var(): Not enough shared memory left")
        );
    }

    #[test]
    fn detach_destroys_object_handle() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let shm = builtin_shm_attach(
            &mut context,
            vec![Value::Int(unique_sysvshm_key(5)), Value::Int(1024)],
            RuntimeSourceSpan::default(),
        )
        .expect("attach");
        assert_eq!(
            builtin_shm_remove(
                &mut context,
                vec![shm.clone()],
                RuntimeSourceSpan::default(),
            )
            .expect("remove"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_shm_detach(
                &mut context,
                vec![shm.clone()],
                RuntimeSourceSpan::default()
            )
            .expect("detach"),
            Value::Bool(true)
        );

        let error = builtin_shm_detach(&mut context, vec![shm], RuntimeSourceSpan::default())
            .expect_err("destroyed handle");
        assert_eq!(
            error.message(),
            "Shared memory block has already been destroyed"
        );
    }

    #[test]
    fn remove_allows_detach_before_destroying_handle() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let shm = builtin_shm_attach(
            &mut context,
            vec![Value::Int(unique_sysvshm_key(6)), Value::Int(1024)],
            RuntimeSourceSpan::default(),
        )
        .expect("attach");

        assert_eq!(
            builtin_shm_remove(
                &mut context,
                vec![shm.clone()],
                RuntimeSourceSpan::default(),
            )
            .expect("remove"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_shm_detach(
                &mut context,
                vec![shm.clone()],
                RuntimeSourceSpan::default(),
            )
            .expect("detach"),
            Value::Bool(true)
        );

        let error = builtin_shm_remove(&mut context, vec![shm], RuntimeSourceSpan::default())
            .expect_err("destroyed handle");
        assert_eq!(
            error.message(),
            "Shared memory block has already been destroyed"
        );
    }
}
