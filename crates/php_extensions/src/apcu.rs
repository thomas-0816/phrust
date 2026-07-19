//! Process-local APCu compatibility helpers.

use php_runtime::api::{
    ApcuState, ArrayKey, ExtensionDescriptor, ExtensionModule, ExtensionStateFactory, PhpArray,
    PhpString, Value, to_bool, to_int, to_string,
};
use php_runtime::api::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use std::any::Any;

pub(crate) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("apcu_add", builtin_apcu_add, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "apcu_clear_cache",
        builtin_apcu_clear_cache,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "apcu_cache_info",
        builtin_apcu_cache_info,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("apcu_dec", builtin_apcu_dec, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "apcu_delete",
        builtin_apcu_delete,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "apcu_enabled",
        builtin_apcu_enabled,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("apcu_entry", builtin_apcu_entry, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "apcu_exists",
        builtin_apcu_exists,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("apcu_fetch", builtin_apcu_fetch, BuiltinCompatibility::Php),
    BuiltinEntry::new("apcu_inc", builtin_apcu_inc, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "apcu_sma_info",
        builtin_apcu_sma_info,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("apcu_store", builtin_apcu_store, BuiltinCompatibility::Php),
];

pub(crate) struct ApcuExtension;

fn create_state() -> Box<dyn Any> {
    Box::new(ApcuState::default())
}

const APCU_STATE_SLOT: &str = match crate::generated::APCU.state_slot {
    Some(name) => name,
    None => "",
};

static DESCRIPTOR: ExtensionDescriptor = ExtensionDescriptor {
    name: crate::generated::APCU.name,
    version: crate::generated::APCU.version,
    dependencies: crate::generated::APCU.dependencies,
    functions: ENTRIES,
    classes: &[],
    constants: &[],
    request_state: Some(ExtensionStateFactory::of::<ApcuState>(
        APCU_STATE_SLOT,
        create_state,
    )),
    capabilities: crate::generated::APCU.capabilities,
    initialize: None,
    shutdown: None,
};

impl ExtensionModule for ApcuExtension {
    fn descriptor(&self) -> &'static ExtensionDescriptor {
        &DESCRIPTOR
    }
}

fn arity_error(name: &str, expected: &str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_ARITY",
        format!("builtin {name} expects {expected}"),
    )
}

fn conversion_error(name: &str, message: String) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_TYPE",
        format!("builtin {name} could not convert value: {message}"),
    )
}

fn apcu_state<'a>(context: &'a mut BuiltinContext<'_>) -> Result<&'a mut ApcuState, BuiltinError> {
    context.apcu_state().ok_or_else(|| {
        BuiltinError::new(
            "E_PHP_RUNTIME_EXTENSION_STATE",
            "APCu extension state is not registered for this request",
        )
    })
}

fn string_arg(name: &str, value: &Value) -> Result<PhpString, BuiltinError> {
    to_string(value).map_err(|message| {
        BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!("builtin {name} expects string-compatible value: {message}"),
        )
    })
}

fn int_arg(name: &str, value: &Value) -> Result<i64, BuiltinError> {
    to_int(value).map_err(|message| {
        BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!("builtin {name} expects int-compatible value: {message}"),
        )
    })
}

fn assign_reference_arg(argument: Option<&Value>, value: Value) {
    if let Some(Value::Reference(reference)) = argument {
        reference.set(value);
    }
}

fn builtin_apcu_enabled(
    _context: &mut BuiltinContext<'_>,
    args: php_runtime::api::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !args.is_empty() {
        return Err(arity_error("apcu_enabled", "zero arguments"));
    }
    Ok(Value::Bool(true))
}

fn builtin_apcu_store(
    context: &mut BuiltinContext<'_>,
    args: php_runtime::api::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("apcu_store", "two or three arguments"));
    }
    let key = string_arg("apcu_store", &args[0])?.as_bytes().to_vec();
    let ttl = args
        .get(2)
        .map(|value| int_arg("apcu_store", value))
        .transpose()?
        .unwrap_or(0);
    apcu_state(context)?.store(key, args[1].clone(), ttl);
    Ok(Value::Bool(true))
}

fn builtin_apcu_add(
    context: &mut BuiltinContext<'_>,
    args: php_runtime::api::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("apcu_add", "two or three arguments"));
    }
    let key = string_arg("apcu_add", &args[0])?.as_bytes().to_vec();
    let ttl = args
        .get(2)
        .map(|value| int_arg("apcu_add", value))
        .transpose()?
        .unwrap_or(0);
    Ok(Value::Bool(apcu_state(context)?.add(
        key,
        args[1].clone(),
        ttl,
    )))
}

fn builtin_apcu_entry(
    context: &mut BuiltinContext<'_>,
    args: php_runtime::api::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("apcu_entry", "two or three arguments"));
    }
    let key = string_arg("apcu_entry", &args[0])?;
    if let Some(value) = apcu_state(context)?.fetch(key.as_bytes()) {
        return Ok(value);
    }
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_CALLBACK",
        "apcu_entry(): callable execution requires VM mediation",
    ))
}

fn builtin_apcu_fetch(
    context: &mut BuiltinContext<'_>,
    args: php_runtime::api::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("apcu_fetch", "one or two arguments"));
    }
    let key = string_arg("apcu_fetch", &args[0])?;
    let value = apcu_state(context)?.fetch(key.as_bytes());
    assign_reference_arg(args.get(1), Value::Bool(value.is_some()));
    Ok(value.unwrap_or(Value::Bool(false)))
}

fn builtin_apcu_exists(
    context: &mut BuiltinContext<'_>,
    args: php_runtime::api::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("apcu_exists", "one argument"));
    }
    let key = string_arg("apcu_exists", &args[0])?;
    Ok(Value::Bool(apcu_state(context)?.exists(key.as_bytes())))
}

fn builtin_apcu_delete(
    context: &mut BuiltinContext<'_>,
    args: php_runtime::api::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("apcu_delete", "one argument"));
    }
    let key = string_arg("apcu_delete", &args[0])?;
    Ok(Value::Bool(apcu_state(context)?.delete(key.as_bytes())))
}

fn builtin_apcu_clear_cache(
    context: &mut BuiltinContext<'_>,
    args: php_runtime::api::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !args.is_empty() {
        return Err(arity_error("apcu_clear_cache", "zero arguments"));
    }
    apcu_state(context)?.clear();
    Ok(Value::Bool(true))
}

fn builtin_apcu_inc(
    context: &mut BuiltinContext<'_>,
    args: php_runtime::api::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    apcu_counter(context, "apcu_inc", args, CounterDirection::Increment)
}

fn builtin_apcu_dec(
    context: &mut BuiltinContext<'_>,
    args: php_runtime::api::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    apcu_counter(context, "apcu_dec", args, CounterDirection::Decrement)
}

fn builtin_apcu_cache_info(
    context: &mut BuiltinContext<'_>,
    args: php_runtime::api::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("apcu_cache_info", "zero or one argument"));
    }
    let limited = optional_bool("apcu_cache_info", args.first())?.unwrap_or(false);
    let stats = apcu_state(context)?.stats();
    let mut result = PhpArray::new();
    result.insert(string_key("num_slots"), Value::Int(1));
    result.insert(string_key("ttl"), Value::Int(0));
    result.insert(string_key("num_hits"), Value::Int(stats.hits as i64));
    result.insert(string_key("num_misses"), Value::Int(stats.misses as i64));
    result.insert(string_key("num_inserts"), Value::Int(stats.inserts as i64));
    result.insert(string_key("num_entries"), Value::Int(stats.entries as i64));
    result.insert(string_key("expunges"), Value::Int(0));
    result.insert(string_key("mem_size"), Value::Int(0));
    result.insert(string_key("memory_type"), Value::string("process-local"));
    if !limited {
        result.insert(string_key("cache_list"), Value::Array(PhpArray::new()));
    }
    Ok(Value::Array(result))
}

fn builtin_apcu_sma_info(
    _context: &mut BuiltinContext<'_>,
    args: php_runtime::api::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("apcu_sma_info", "zero or one argument"));
    }
    let limited = optional_bool("apcu_sma_info", args.first())?.unwrap_or(false);
    let mut result = PhpArray::new();
    result.insert(string_key("num_seg"), Value::Int(1));
    result.insert(string_key("seg_size"), Value::Int(0));
    result.insert(string_key("avail_mem"), Value::Int(0));
    if !limited {
        result.insert(string_key("block_lists"), Value::Array(PhpArray::new()));
    }
    Ok(Value::Array(result))
}

#[derive(Clone, Copy)]
enum CounterDirection {
    Increment,
    Decrement,
}

fn apcu_counter(
    context: &mut BuiltinContext<'_>,
    function: &'static str,
    args: php_runtime::api::BuiltinArgs,
    direction: CounterDirection,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 4 {
        return Err(arity_error(function, "one to four arguments"));
    }
    let key = string_arg(function, &args[0])?;
    let step = args
        .get(1)
        .map(|value| int_arg(function, value))
        .transpose()?
        .unwrap_or(1);
    let _ttl = args
        .get(3)
        .map(|value| int_arg(function, value))
        .transpose()?
        .unwrap_or(0);
    let next = match direction {
        CounterDirection::Increment => apcu_state(context)?.increment(key.as_bytes(), step),
        CounterDirection::Decrement => apcu_state(context)?.decrement(key.as_bytes(), step),
    };
    assign_reference_arg(args.get(2), Value::Bool(next.is_some()));
    Ok(next.map(Value::Int).unwrap_or(Value::Bool(false)))
}

fn optional_bool(
    function: &'static str,
    value: Option<&Value>,
) -> Result<Option<bool>, BuiltinError> {
    value
        .map(|value| to_bool(value).map_err(|message| conversion_error(function, message)))
        .transpose()
}

fn string_key(value: &str) -> ArrayKey {
    ArrayKey::String(PhpString::from_test_str(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_runtime::api::{ExtensionStateLayoutBuilder, OutputBuffer};

    fn call_with_context(name: &str, args: php_runtime::api::BuiltinArgs, context: &mut BuiltinContext<'_>) -> Value {
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .expect("entry")
            .function()(context, args, RuntimeSourceSpan::default())
        .expect("builtin succeeds")
    }

    #[test]
    fn missing_registered_state_fails_closed() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let error = ENTRIES
            .iter()
            .find(|entry| entry.name() == "apcu_store")
            .expect("entry")
            .function()(
            &mut context,
            vec![Value::string("key"), Value::Int(1)],
            RuntimeSourceSpan::default(),
        )
        .expect_err("unregistered APCu state must not use a fallback owner");
        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_EXTENSION_STATE");
    }

    #[test]
    fn counters_and_info_cover_process_local_cache_slice() {
        let mut output = OutputBuffer::default();
        let mut layout = ExtensionStateLayoutBuilder::new();
        let slot = layout.register(ApcuState::isolated).expect("APCu slot");
        let mut request_state = layout.build().create_request_state();
        let mut context = BuiltinContext::new(&mut output);
        context.set_apcu_request_state(&mut request_state, slot);

        assert_eq!(
            call_with_context(
                "apcu_store",
                vec![Value::string("count"), Value::Int(4)],
                &mut context,
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_with_context("apcu_inc", vec![Value::string("count")], &mut context),
            Value::Int(5)
        );
        assert_eq!(
            call_with_context(
                "apcu_dec",
                vec![Value::string("count"), Value::Int(2)],
                &mut context,
            ),
            Value::Int(3)
        );

        let Value::Array(info) = call_with_context("apcu_cache_info", vec![], &mut context) else {
            panic!("expected cache info array");
        };
        assert_eq!(info.get(&string_key("num_entries")), Some(&Value::Int(1)));
        assert_eq!(info.get(&string_key("num_hits")), Some(&Value::Int(2)));
        assert_eq!(info.get(&string_key("num_inserts")), Some(&Value::Int(1)));

        let Value::Array(sma) = call_with_context("apcu_sma_info", vec![], &mut context) else {
            panic!("expected sma info array");
        };
        assert_eq!(sma.get(&string_key("num_seg")), Some(&Value::Int(1)));
    }

    #[test]
    fn default_state_is_shared_across_context_handles() {
        let key = b"__phrust_apcu_process_shared_unit_test".to_vec();
        let mut first = ApcuState::default();
        let mut second = ApcuState::default();
        first.delete(&key);

        first.store(key.clone(), Value::string("shared"), 0);

        assert_eq!(second.fetch(&key), Some(Value::string("shared")));
        second.delete(&key);
    }

    #[test]
    fn ttl_expiry_uses_controllable_clock_for_isolated_state() {
        let base = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000);
        let mut apcu = ApcuState::isolated_at(base);

        apcu.store(b"ttl".to_vec(), Value::string("value"), 10);
        assert_eq!(apcu.fetch(b"ttl"), Some(Value::string("value")));

        apcu.set_test_now(base + std::time::Duration::from_secs(11));
        assert_eq!(apcu.fetch(b"ttl"), None);
    }
}
