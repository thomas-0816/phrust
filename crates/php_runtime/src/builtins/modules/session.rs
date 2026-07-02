//! Session builtin registry slice.

use super::core::*;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{PHP_SESSION_NONE, Value};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "session_cache_expire",
        builtin_session_cache_expire,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_cache_limiter",
        builtin_session_cache_limiter,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_commit",
        builtin_session_write_close,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_destroy",
        builtin_session_destroy,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("session_id", builtin_session_id, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "session_module_name",
        builtin_session_module_name,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_name",
        builtin_session_name,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_save_path",
        builtin_session_save_path,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_start",
        builtin_session_start,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_status",
        builtin_session_status,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "session_write_close",
        builtin_session_write_close,
        BuiltinCompatibility::Php,
    ),
];

pub(in crate::builtins::modules) fn builtin_session_cache_expire(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error(
            "session_cache_expire",
            "zero or one argument(s)",
        ));
    }
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_cache_expire"));
    };
    let previous = state.cache_expire();
    if let Some(value) = args.first()
        && !matches!(deref_value(value), Value::Null)
    {
        state.replace_cache_expire(int_arg("session_cache_expire", value)?);
    }
    Ok(Value::Int(previous))
}

pub(in crate::builtins::modules) fn builtin_session_cache_limiter(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error(
            "session_cache_limiter",
            "zero or one argument(s)",
        ));
    }
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_cache_limiter"));
    };
    let previous = state.cache_limiter().to_owned();
    if let Some(value) = args.first()
        && !matches!(deref_value(value), Value::Null)
    {
        state.replace_cache_limiter(string_arg("session_cache_limiter", value)?.to_string_lossy());
    }
    Ok(Value::string(previous))
}

pub(in crate::builtins::modules) fn builtin_session_status(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("session_status", &args, 0)?;
    let status = context
        .session_state()
        .map_or(PHP_SESSION_NONE, |state| state.status());
    Ok(Value::Int(status))
}

pub(in crate::builtins::modules) fn builtin_session_name(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("session_name", "zero or one argument(s)"));
    }
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_name"));
    };
    let previous = state.name().to_owned();
    if let Some(name) = args.first()
        && !matches!(deref_value(name), Value::Null)
    {
        state.replace_name(string_arg("session_name", name)?.to_string_lossy());
    }
    Ok(Value::string(previous))
}

pub(in crate::builtins::modules) fn builtin_session_module_name(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error(
            "session_module_name",
            "zero or one argument(s)",
        ));
    }
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_module_name"));
    };
    let previous = state.module_name().to_owned();
    if let Some(name) = args.first()
        && !matches!(deref_value(name), Value::Null)
    {
        state.replace_module_name(string_arg("session_module_name", name)?.to_string_lossy());
    }
    Ok(Value::string(previous))
}

pub(in crate::builtins::modules) fn builtin_session_save_path(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("session_save_path", "zero or one argument(s)"));
    }
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_save_path"));
    };
    let previous = state.save_path().to_owned();
    if let Some(path) = args.first()
        && !matches!(deref_value(path), Value::Null)
    {
        state.replace_save_path(string_arg("session_save_path", path)?.to_string_lossy());
    }
    Ok(Value::string(previous))
}

pub(in crate::builtins::modules) fn builtin_session_id(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("session_id", "zero or one argument(s)"));
    }
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_id"));
    };
    let previous = state.id().to_owned();
    if let Some(id) = args.first()
        && !matches!(deref_value(id), Value::Null)
    {
        state.replace_id(string_arg("session_id", id)?.to_string_lossy());
    }
    Ok(Value::string(previous))
}

pub(in crate::builtins::modules) fn builtin_session_start(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("session_start", "zero or one argument(s)"));
    }
    if let Some(options) = args.first()
        && !matches!(deref_value(options), Value::Array(_))
    {
        return Err(type_error("session_start", "array", options));
    }
    let needs_lazy_load = {
        let Some(state) = context.session_state() else {
            return Err(session_context_error("session_start"));
        };
        state.needs_lazy_load()
    };
    if needs_lazy_load {
        context
            .load_pending_session_data()
            .map_err(|message| session_store_error("session_start", message))?;
    }
    {
        let Some(state) = context.session_state() else {
            return Err(session_context_error("session_start"));
        };
        state.start();
    }
    context.sync_session_global_from_state();
    Ok(Value::Bool(true))
}

pub(in crate::builtins::modules) fn builtin_session_destroy(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("session_destroy", &args, 0)?;
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_destroy"));
    };
    let destroyed = state.destroy();
    context.sync_session_global_from_state();
    Ok(Value::Bool(destroyed))
}

pub(in crate::builtins::modules) fn builtin_session_write_close(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("session_write_close", &args, 0)?;
    let Some(state) = context.session_state() else {
        return Err(session_context_error("session_write_close"));
    };
    Ok(Value::Bool(state.write_close()))
}

fn session_context_error(function: &str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_SESSION_CONTEXT_REQUIRED",
        format!("{function}() requires VM request-local session state"),
    )
}

fn session_store_error(function: &str, message: String) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_SESSION_STORE_UNAVAILABLE",
        format!("{function}() could not load session data: {message}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ArrayKey, OutputBuffer, PhpArray, PhpString, ReferenceCell, SessionLoadCallback,
        SessionState,
    };

    fn context_with_session<'a>(
        output: &'a mut OutputBuffer,
        state: &'a mut SessionState,
        global: ReferenceCell,
    ) -> BuiltinContext<'a> {
        let mut context = BuiltinContext::new(output);
        context.set_session_state(state, global);
        context
    }

    #[test]
    fn session_builtins_track_cli_state() {
        let mut output = OutputBuffer::new();
        let mut state = SessionState::default();
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let mut context = context_with_session(&mut output, &mut state, global.clone());

        assert_eq!(
            builtin_session_status(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("status"),
            Value::Int(PHP_SESSION_NONE)
        );
        assert_eq!(
            builtin_session_name(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("name"),
            Value::string("PHPSESSID")
        );
        assert_eq!(
            builtin_session_cache_expire(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("cache expire"),
            Value::Int(180)
        );
        assert_eq!(
            builtin_session_cache_limiter(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("cache limiter"),
            Value::string("nocache")
        );
        assert_eq!(
            builtin_session_module_name(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("module name"),
            Value::string("files")
        );
        assert_eq!(
            builtin_session_save_path(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("save path"),
            Value::string("")
        );
        assert_eq!(
            builtin_session_id(
                &mut context,
                vec![Value::string("local")],
                RuntimeSourceSpan::default()
            )
            .expect("id"),
            Value::string("")
        );
        assert_eq!(
            builtin_session_start(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("start"),
            Value::Bool(true)
        );
        assert_eq!(global.get(), Value::Array(crate::PhpArray::new()));
        assert_eq!(
            builtin_session_write_close(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("write close"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_session_status(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("status after write close"),
            Value::Int(PHP_SESSION_NONE)
        );
        assert_eq!(
            builtin_session_id(&mut context, Vec::new(), RuntimeSourceSpan::default()).expect("id"),
            Value::string("local")
        );
        assert_eq!(
            builtin_session_start(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("restart"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_session_destroy(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("destroy"),
            Value::Bool(true)
        );
    }

    #[test]
    fn session_builtins_use_seeded_web_state() {
        let mut seeded = PhpArray::new();
        seeded.insert(ArrayKey::String(PhpString::from("n")), Value::Int(7));
        let mut output = OutputBuffer::new();
        let mut state = SessionState::seeded(
            "APPSESSID".to_string(),
            "incoming123".to_string(),
            seeded.clone(),
            Some("generated456".to_string()),
        );
        let global = ReferenceCell::new(Value::Array(seeded));
        let mut context = context_with_session(&mut output, &mut state, global.clone());

        assert_eq!(
            builtin_session_name(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("name"),
            Value::string("APPSESSID")
        );
        assert_eq!(
            builtin_session_start(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("start"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_session_id(&mut context, Vec::new(), RuntimeSourceSpan::default()).expect("id"),
            Value::string("incoming123")
        );
        assert_eq!(
            global.get(),
            Value::Array({
                let mut array = PhpArray::new();
                array.insert(ArrayKey::String(PhpString::from("n")), Value::Int(7));
                array
            })
        );
        assert!(!state.newly_created());
    }

    #[test]
    fn session_start_loads_lazy_seeded_web_state() {
        let mut output = OutputBuffer::new();
        let mut state = SessionState::seeded_lazy(
            "APPSESSID".to_string(),
            "incoming123".to_string(),
            Some("generated456".to_string()),
        );
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let loader = SessionLoadCallback::new(|id| {
            assert_eq!(id, "incoming123");
            let mut data = PhpArray::new();
            data.insert(ArrayKey::String(PhpString::from("n")), Value::Int(7));
            Ok(data)
        });
        let mut context = context_with_session(&mut output, &mut state, global.clone());
        context.set_session_loader(Some(&loader));

        assert_eq!(
            builtin_session_start(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("start"),
            Value::Bool(true)
        );
        assert_eq!(
            global.get(),
            Value::Array({
                let mut array = PhpArray::new();
                array.insert(ArrayKey::String(PhpString::from("n")), Value::Int(7));
                array
            })
        );
        assert!(!state.newly_created());
    }
}
