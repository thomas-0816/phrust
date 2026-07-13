//! Deterministic `shmop` shared-memory compatibility slice.

use super::core::{argument_type_error, argument_value_error, arity_error, int_arg, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ClassEntry, ClassFlags, ObjectRef, Value, normalize_class_name};

const SHMOP_CLASS: &str = "Shmop";
const SHMOP_ID_PROPERTY: &str = "__shmop_id";
const SHMOP_READ_ONLY_PROPERTY: &str = "__shmop_read_only";

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "shmop_close",
        builtin_shmop_close,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "shmop_delete",
        builtin_shmop_delete,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("shmop_open", builtin_shmop_open, BuiltinCompatibility::Php),
    BuiltinEntry::new("shmop_read", builtin_shmop_read, BuiltinCompatibility::Php),
    BuiltinEntry::new("shmop_size", builtin_shmop_size, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "shmop_write",
        builtin_shmop_write,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_shmop_open(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("shmop_open", &args, 4)?;
    let key = int_arg("shmop_open", &args[0])?;
    let mode = string_arg("shmop_open", &args[1])?.to_string_lossy();
    let permissions = int_arg("shmop_open", &args[2])?;
    let size = int_arg("shmop_open", &args[3])?;
    let mode = shmop_mode(&mode)?;
    if matches!(mode, 'c' | 'n') && size <= 0 {
        return Err(argument_value_error(
            "shmop_open",
            "#4 ($size)",
            "must be greater than 0 for the \"c\" and \"n\" access modes",
        ));
    }
    let size = size.max(0) as usize;
    let Some(segment_id) = context.shmop_state().open(key, mode, permissions, size) else {
        context.php_warning(
            "E_PHP_RUNTIME_SHMOP_OPEN",
            format!(
                "shmop_open(): Unable to attach or create shared memory segment \"{}\"",
                permissions
            ),
            span,
        );
        return Ok(Value::Bool(false));
    };
    Ok(Value::Object(shmop_object(segment_id, matches!(mode, 'a'))))
}

fn builtin_shmop_read(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("shmop_read", &args, 3)?;
    let segment_id = shmop_segment_id("shmop_read", &args[0])?;
    let offset = int_arg("shmop_read", &args[1])?;
    let size = int_arg("shmop_read", &args[2])?;
    let Some(segment) = context.shmop_state().segment(segment_id) else {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_SHMOP_INVALID",
            "shmop_read(): Shmop object is no longer valid",
        ));
    };
    if offset < 0 || offset as usize > segment.size() {
        return Err(argument_value_error(
            "shmop_read",
            "#2 ($offset)",
            "must be between 0 and the segment size",
        ));
    }
    if size < 0 {
        return Err(argument_value_error(
            "shmop_read",
            "#3 ($size)",
            "is out of range",
        ));
    }
    Ok(Value::string(segment.read(offset as usize, size as usize)))
}

fn builtin_shmop_size(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("shmop_size", &args, 1)?;
    let segment_id = shmop_segment_id("shmop_size", &args[0])?;
    let Some(segment) = context.shmop_state().segment(segment_id) else {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_SHMOP_INVALID",
            "shmop_size(): Shmop object is no longer valid",
        ));
    };
    Ok(Value::Int(segment.size() as i64))
}

fn builtin_shmop_write(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("shmop_write", &args, 3)?;
    let object = shmop_object_arg("shmop_write", &args[0])?;
    if matches!(
        object.get_property(SHMOP_READ_ONLY_PROPERTY),
        Some(Value::Bool(true))
    ) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_SHMOP_READ_ONLY",
            "Read-only segment cannot be written",
        ));
    }
    let segment_id = shmop_segment_id_from_object(&object).ok_or_else(|| {
        BuiltinError::new(
            "E_PHP_RUNTIME_SHMOP_INVALID",
            "shmop_write(): Shmop object is no longer valid",
        )
    })?;
    let data = string_arg("shmop_write", &args[1])?;
    let offset = int_arg("shmop_write", &args[2])?;
    let Some(segment) = context.shmop_state().segment_mut(segment_id) else {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_SHMOP_INVALID",
            "shmop_write(): Shmop object is no longer valid",
        ));
    };
    if offset < 0 || offset as usize > segment.size() {
        return Err(argument_value_error(
            "shmop_write",
            "#3 ($offset)",
            "is out of range",
        ));
    }
    Ok(Value::Int(
        segment.write(offset as usize, data.as_bytes()) as i64
    ))
}

fn builtin_shmop_delete(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("shmop_delete", &args, 1)?;
    let segment_id = shmop_segment_id("shmop_delete", &args[0])?;
    Ok(Value::Bool(context.shmop_state().delete(segment_id)))
}

fn builtin_shmop_close(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_exact("shmop_close", &args, 1)?;
    let _ = shmop_segment_id("shmop_close", &args[0])?;
    context.php_deprecation(
        "E_PHP_RUNTIME_SHMOP_CLOSE_DEPRECATED",
        "shmop_close(): Function shmop_close() is deprecated",
        span,
    );
    Ok(Value::Null)
}

fn expect_exact(name: &str, args: &[Value], expected: usize) -> Result<(), BuiltinError> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(arity_error(name, &format!("{expected} argument(s)")))
    }
}

fn shmop_mode(mode: &str) -> Result<char, BuiltinError> {
    let mut chars = mode.chars();
    let Some(mode) = chars.next() else {
        return Err(invalid_access_mode());
    };
    if chars.next().is_some() || !matches!(mode, 'a' | 'c' | 'n' | 'w') {
        return Err(invalid_access_mode());
    }
    Ok(mode)
}

fn invalid_access_mode() -> BuiltinError {
    argument_value_error("shmop_open", "#2 ($mode)", "must be a valid access mode")
}

fn shmop_segment_id(name: &str, value: &Value) -> Result<i64, BuiltinError> {
    let object = shmop_object_arg(name, value)?;
    shmop_segment_id_from_object(&object).ok_or_else(|| {
        BuiltinError::new(
            "E_PHP_RUNTIME_SHMOP_INVALID",
            format!("{name}(): Shmop object is no longer valid"),
        )
    })
}

fn shmop_object_arg(name: &str, value: &Value) -> Result<ObjectRef, BuiltinError> {
    let Value::Object(object) = value else {
        return Err(argument_type_error(name, "#1 ($shmop)", SHMOP_CLASS, value));
    };
    if normalize_class_name(&object.class_name()) != "shmop" {
        return Err(argument_type_error(name, "#1 ($shmop)", SHMOP_CLASS, value));
    }
    Ok(object.clone())
}

fn shmop_segment_id_from_object(object: &ObjectRef) -> Option<i64> {
    match object.get_property(SHMOP_ID_PROPERTY) {
        Some(Value::Int(id)) if id > 0 => Some(id),
        _ => None,
    }
}

fn shmop_object(segment_id: i64, read_only: bool) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&shmop_runtime_class(), SHMOP_CLASS);
    object.set_property(SHMOP_ID_PROPERTY, Value::Int(segment_id));
    object.set_property(SHMOP_READ_ONLY_PROPERTY, Value::Bool(read_only));
    object
}

fn shmop_runtime_class() -> ClassEntry {
    ClassEntry {
        name: "shmop".to_owned().into(),
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
    fn sysv_segments_share_by_key_and_private_key_zero_is_isolated() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        // Remove any key-42 segment a crashed previous run left behind;
        // the end-of-test delete cannot run when the process is killed
        // mid-test.
        if let Ok(stale) = builtin_shmop_open(
            &mut context,
            vec![
                Value::Int(42),
                Value::string("c"),
                Value::Int(0o600),
                Value::Int(16),
            ],
            RuntimeSourceSpan::default(),
        ) {
            let _ = builtin_shmop_delete(&mut context, vec![stale], RuntimeSourceSpan::default());
        }
        let created = builtin_shmop_open(
            &mut context,
            vec![
                Value::Int(42),
                Value::string("n"),
                Value::Int(0o600),
                Value::Int(16),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("create segment");
        assert_eq!(
            builtin_shmop_write(
                &mut context,
                vec![created.clone(), Value::string("abcd"), Value::Int(0)],
                RuntimeSourceSpan::default(),
            )
            .expect("write"),
            Value::Int(4)
        );
        let opened = builtin_shmop_open(
            &mut context,
            vec![
                Value::Int(42),
                Value::string("a"),
                Value::Int(0o600),
                Value::Int(16),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("read-only open");
        assert_eq!(
            builtin_shmop_read(
                &mut context,
                vec![opened.clone(), Value::Int(0), Value::Int(4)],
                RuntimeSourceSpan::default(),
            )
            .expect("read"),
            Value::string("abcd")
        );
        assert!(
            builtin_shmop_write(
                &mut context,
                vec![opened, Value::string("x"), Value::Int(0)],
                RuntimeSourceSpan::default(),
            )
            .is_err()
        );

        let private_a = builtin_shmop_open(
            &mut context,
            vec![
                Value::Int(0),
                Value::string("c"),
                Value::Int(0o600),
                Value::Int(4),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("private a");
        let private_b = builtin_shmop_open(
            &mut context,
            vec![
                Value::Int(0),
                Value::string("c"),
                Value::Int(0o600),
                Value::Int(4),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("private b");
        assert_ne!(
            shmop_segment_id("shmop_size", &private_a).expect("id a"),
            shmop_segment_id("shmop_size", &private_b).expect("id b")
        );
        // Delete every created segment: SysV shared memory outlives the
        // process, and the keyed segment otherwise makes the exclusive
        // create above fail on every subsequent run.
        for segment in [created, private_a, private_b] {
            assert_eq!(
                builtin_shmop_delete(&mut context, vec![segment], RuntimeSourceSpan::default())
                    .expect("delete segment"),
                Value::Bool(true)
            );
        }
    }
}
