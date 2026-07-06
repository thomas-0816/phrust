//! Runtime shim for the built-in `Redis` class, extracted from the VM module.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::result_large_err)]

use super::prelude::*;

pub(super) fn is_redis_runtime_class(class_name: &str) -> bool {
    normalize_class_name(class_name) == "redis"
}

pub(super) fn internal_redis_instanceof(object_class: &str, target_class: &str) -> Option<bool> {
    if !is_redis_runtime_class(object_class) {
        return None;
    }
    Some(normalize_class_name(target_class) == "redis")
}

const REDIS_CONNECTED_PROPERTY: &str = "__redis_connected";
const REDIS_DB_PROPERTY: &str = "__redis_db";
const REDIS_STORE_PROPERTY: &str = "__redis_store";
const REDIS_OPTIONS_PROPERTY: &str = "__redis_options";

pub(super) fn new_redis_object(
    class_name: &str,
    args: Vec<CallArgument>,
) -> Result<ObjectRef, String> {
    if !is_redis_runtime_class(class_name) {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"
        ));
    }
    let values = call_args_to_positional("Redis::__construct", args)?;
    validate_redis_arg_count("Redis::__construct", values.len(), 0, 0)?;
    let object = ObjectRef::new_with_display_name(&redis_runtime_class(), "Redis");
    redis_reset_object(&object);
    Ok(object)
}

pub(super) fn redis_runtime_class() -> RuntimeClassEntry {
    RuntimeClassEntry {
        name: "redis".to_owned(),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: RuntimeClassFlags::default(),
    }
}

pub(super) fn redis_reset_object(object: &ObjectRef) {
    object.set_property(REDIS_CONNECTED_PROPERTY, Value::Bool(false));
    object.set_property(REDIS_DB_PROPERTY, Value::Int(0));
    object.set_property(REDIS_STORE_PROPERTY, Value::Array(PhpArray::new()));
    object.set_property(REDIS_OPTIONS_PROPERTY, Value::Array(PhpArray::new()));
}

pub(super) fn call_redis_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
) -> Result<Value, String> {
    let method = normalize_method_name(method);
    let values = call_args_to_positional(&format!("Redis::{method}"), args)?;
    match method.as_str() {
        "__construct" => {
            validate_redis_arg_count("Redis::__construct", values.len(), 0, 0)?;
            redis_reset_object(object);
            Ok(Value::Null)
        }
        "connect" | "pconnect" => {
            validate_redis_arg_count("Redis::connect", values.len(), 1, 6)?;
            object.set_property(REDIS_CONNECTED_PROPERTY, Value::Bool(true));
            Ok(Value::Bool(true))
        }
        "close" => {
            validate_redis_arg_count("Redis::close", values.len(), 0, 0)?;
            object.set_property(REDIS_CONNECTED_PROPERTY, Value::Bool(false));
            Ok(Value::Bool(true))
        }
        "ping" => {
            validate_redis_arg_count("Redis::ping", values.len(), 0, 1)?;
            Ok(Value::string("+PONG"))
        }
        "isconnected" | "is_connected" => {
            validate_redis_arg_count("Redis::isConnected", values.len(), 0, 0)?;
            Ok(object
                .get_property(REDIS_CONNECTED_PROPERTY)
                .unwrap_or(Value::Bool(false)))
        }
        "auth" => {
            validate_redis_arg_count("Redis::auth", values.len(), 1, 1)?;
            Ok(Value::Bool(true))
        }
        "select" => {
            validate_redis_arg_count("Redis::select", values.len(), 1, 1)?;
            object.set_property(REDIS_DB_PROPERTY, Value::Int(to_int(&values[0])?));
            Ok(Value::Bool(true))
        }
        "set" => redis_set(object, &values),
        "setex" => redis_setex(object, &values),
        "setnx" => redis_setnx(object, &values),
        "get" => redis_get(object, &values),
        "mget" | "getmultiple" => redis_mget(object, &values),
        "mset" => redis_mset(object, &values),
        "del" | "delete" | "unlink" => redis_del(object, &values),
        "exists" => redis_exists(object, &values),
        "expire" | "pexpire" | "persist" => redis_key_bool_result(object, &values, "Redis::expire"),
        "ttl" | "pttl" => redis_ttl(object, &values),
        "incr" => redis_counter(object, &values, 1),
        "incrby" => redis_counter_by(object, &values, 1),
        "decr" => redis_counter(object, &values, -1),
        "decrby" => redis_counter_by(object, &values, -1),
        "hset" => redis_hset(object, &values),
        "hget" => redis_hget(object, &values),
        "hgetall" => redis_hgetall(object, &values),
        "hdel" => redis_hdel(object, &values),
        "hexists" => redis_hexists(object, &values),
        "lpush" => redis_list_push(object, &values, true),
        "rpush" => redis_list_push(object, &values, false),
        "lpop" => redis_list_pop(object, &values, true),
        "rpop" => redis_list_pop(object, &values, false),
        "llen" => redis_list_len(object, &values),
        "sadd" => redis_sadd(object, &values),
        "smembers" => redis_smembers(object, &values),
        "sismember" | "scontains" => redis_sismember(object, &values),
        "srem" | "sremove" => redis_srem(object, &values),
        "zadd" => redis_zadd(object, &values),
        "zrange" => redis_zrange(object, &values),
        "flushdb" | "flushall" => {
            validate_redis_arg_count("Redis::flushDB", values.len(), 0, 1)?;
            object.set_property(REDIS_STORE_PROPERTY, Value::Array(PhpArray::new()));
            Ok(Value::Bool(true))
        }
        "multi" | "pipeline" => {
            validate_redis_arg_count("Redis::multi", values.len(), 0, 1)?;
            Ok(Value::Object(object.clone()))
        }
        "exec" => {
            validate_redis_arg_count("Redis::exec", values.len(), 0, 0)?;
            Ok(Value::Array(PhpArray::new()))
        }
        "discard" => {
            validate_redis_arg_count("Redis::discard", values.len(), 0, 0)?;
            Ok(Value::Bool(true))
        }
        "scan" => redis_scan(object, &values),
        "setoption" => redis_set_option(object, &values),
        "getoption" => redis_get_option(object, &values),
        other => Err(format!(
            "E_PHP_VM_REDIS_METHOD_GAP: method Redis::{other} is not implemented in the deterministic Redis fake backend"
        )),
    }
}

pub(super) fn validate_redis_arg_count(
    function: &str,
    actual: usize,
    min: usize,
    max: usize,
) -> Result<(), String> {
    if actual < min || actual > max {
        return Err(format!(
            "E_PHP_VM_REDIS_ARG_COUNT: {function} expects {min}..{max} argument(s), {actual} given"
        ));
    }
    Ok(())
}

pub(super) fn redis_store(object: &ObjectRef) -> PhpArray {
    match object.get_property(REDIS_STORE_PROPERTY) {
        Some(Value::Array(array)) => array,
        _ => PhpArray::new(),
    }
}

pub(super) fn redis_set_store(object: &ObjectRef, store: PhpArray) {
    object.set_property(REDIS_STORE_PROPERTY, Value::Array(store));
}

pub(super) fn redis_key(value: &Value) -> Result<ArrayKey, String> {
    Ok(ArrayKey::String(PhpString::from_bytes(
        to_string(value)?.as_bytes().to_vec(),
    )))
}

pub(super) fn redis_array_property(value: Option<&Value>) -> PhpArray {
    match value {
        Some(Value::Array(array)) => array.clone(),
        _ => PhpArray::new(),
    }
}

pub(super) fn redis_array_value_entries(
    value: &Value,
    function: &str,
) -> Result<Vec<(ArrayKey, Value)>, String> {
    let Value::Array(array) = value else {
        return Err(format!(
            "E_PHP_VM_REDIS_TYPE_ERROR: {function} expects array, {} given",
            value_type_name(value)
        ));
    };
    Ok(array
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect())
}

pub(super) fn redis_set(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::set", values.len(), 2, 5)?;
    let mut store = redis_store(object);
    store.insert(redis_key(&values[0])?, values[1].clone());
    redis_set_store(object, store);
    Ok(Value::Bool(true))
}

pub(super) fn redis_setex(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::setex", values.len(), 3, 3)?;
    let mut store = redis_store(object);
    store.insert(redis_key(&values[0])?, values[2].clone());
    redis_set_store(object, store);
    Ok(Value::Bool(true))
}

pub(super) fn redis_setnx(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::setnx", values.len(), 2, 2)?;
    let mut store = redis_store(object);
    let key = redis_key(&values[0])?;
    if store.get(&key).is_some() {
        return Ok(Value::Bool(false));
    }
    store.insert(key, values[1].clone());
    redis_set_store(object, store);
    Ok(Value::Bool(true))
}

pub(super) fn redis_get(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::get", values.len(), 1, 1)?;
    let store = redis_store(object);
    Ok(store
        .get(&redis_key(&values[0])?)
        .cloned()
        .unwrap_or(Value::Bool(false)))
}

pub(super) fn redis_mget(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::mget", values.len(), 1, 1)?;
    let keys = redis_array_value_entries(&values[0], "Redis::mget")?;
    let store = redis_store(object);
    let result = keys
        .into_iter()
        .map(|(_, key)| {
            redis_key(&key)
                .map(|redis_key| store.get(&redis_key).cloned().unwrap_or(Value::Bool(false)))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::packed_array(result))
}

pub(super) fn redis_mset(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::mset", values.len(), 1, 1)?;
    let mut store = redis_store(object);
    for (key, value) in redis_array_value_entries(&values[0], "Redis::mset")? {
        let key = match key {
            ArrayKey::Int(index) => ArrayKey::String(PhpString::from(index.to_string().as_str())),
            ArrayKey::String(name) => ArrayKey::String(name),
        };
        store.insert(key, value);
    }
    redis_set_store(object, store);
    Ok(Value::Bool(true))
}

pub(super) fn redis_del(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::del", values.len(), 1, usize::MAX)?;
    let mut keys = Vec::new();
    if values.len() == 1 && matches!(values[0], Value::Array(_)) {
        keys.extend(
            redis_array_value_entries(&values[0], "Redis::del")?
                .into_iter()
                .map(|(_, value)| value),
        );
    } else {
        keys.extend(values.iter().cloned());
    }
    let mut store = redis_store(object);
    let mut removed = 0i64;
    for key in keys {
        if store.remove(&redis_key(&key)?).is_some() {
            removed += 1;
        }
    }
    redis_set_store(object, store);
    Ok(Value::Int(removed))
}

pub(super) fn redis_exists(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::exists", values.len(), 1, usize::MAX)?;
    let store = redis_store(object);
    let mut count = 0i64;
    for value in values {
        if store.get(&redis_key(value)?).is_some() {
            count += 1;
        }
    }
    Ok(Value::Int(count))
}

pub(super) fn redis_key_bool_result(
    object: &ObjectRef,
    values: &[Value],
    function: &str,
) -> Result<Value, String> {
    validate_redis_arg_count(function, values.len(), 1, 3)?;
    let store = redis_store(object);
    Ok(Value::Bool(store.get(&redis_key(&values[0])?).is_some()))
}

pub(super) fn redis_ttl(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::ttl", values.len(), 1, 1)?;
    let store = redis_store(object);
    Ok(Value::Int(
        if store.get(&redis_key(&values[0])?).is_some() {
            -1
        } else {
            -2
        },
    ))
}

pub(super) fn redis_counter(
    object: &ObjectRef,
    values: &[Value],
    delta: i64,
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::counter", values.len(), 1, 1)?;
    redis_counter_delta(object, &values[0], delta)
}

pub(super) fn redis_counter_by(
    object: &ObjectRef,
    values: &[Value],
    direction: i64,
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::counterBy", values.len(), 2, 2)?;
    redis_counter_delta(object, &values[0], to_int(&values[1])? * direction)
}

pub(super) fn redis_counter_delta(
    object: &ObjectRef,
    key_value: &Value,
    delta: i64,
) -> Result<Value, String> {
    let mut store = redis_store(object);
    let key = redis_key(key_value)?;
    let current = store.get(&key).map(to_int).transpose()?.unwrap_or(0);
    let next = current
        .checked_add(delta)
        .ok_or_else(|| "E_PHP_VM_REDIS_COUNTER_OVERFLOW: Redis counter overflow".to_owned())?;
    store.insert(key, Value::Int(next));
    redis_set_store(object, store);
    Ok(Value::Int(next))
}

pub(super) fn redis_hset(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::hSet", values.len(), 3, 3)?;
    let mut store = redis_store(object);
    let key = redis_key(&values[0])?;
    let field = redis_key(&values[1])?;
    let mut hash = redis_array_property(store.get(&key));
    let is_new = hash.get(&field).is_none();
    hash.insert(field, values[2].clone());
    store.insert(key, Value::Array(hash));
    redis_set_store(object, store);
    Ok(Value::Int(i64::from(is_new)))
}

pub(super) fn redis_hget(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::hGet", values.len(), 2, 2)?;
    let store = redis_store(object);
    let hash = redis_array_property(store.get(&redis_key(&values[0])?));
    Ok(hash
        .get(&redis_key(&values[1])?)
        .cloned()
        .unwrap_or(Value::Bool(false)))
}

pub(super) fn redis_hgetall(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::hGetAll", values.len(), 1, 1)?;
    let store = redis_store(object);
    Ok(Value::Array(redis_array_property(
        store.get(&redis_key(&values[0])?),
    )))
}

pub(super) fn redis_hdel(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::hDel", values.len(), 2, usize::MAX)?;
    let mut store = redis_store(object);
    let key = redis_key(&values[0])?;
    let mut hash = redis_array_property(store.get(&key));
    let mut removed = 0i64;
    for field in &values[1..] {
        if hash.remove(&redis_key(field)?).is_some() {
            removed += 1;
        }
    }
    store.insert(key, Value::Array(hash));
    redis_set_store(object, store);
    Ok(Value::Int(removed))
}

pub(super) fn redis_hexists(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::hExists", values.len(), 2, 2)?;
    let store = redis_store(object);
    let hash = redis_array_property(store.get(&redis_key(&values[0])?));
    Ok(Value::Bool(hash.get(&redis_key(&values[1])?).is_some()))
}

pub(super) fn redis_list_push(
    object: &ObjectRef,
    values: &[Value],
    left: bool,
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::listPush", values.len(), 2, usize::MAX)?;
    let mut store = redis_store(object);
    let key = redis_key(&values[0])?;
    let mut elements = redis_array_property(store.get(&key))
        .iter()
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    for value in &values[1..] {
        if left {
            elements.insert(0, value.clone());
        } else {
            elements.push(value.clone());
        }
    }
    let len = elements.len() as i64;
    store.insert(key, Value::packed_array(elements));
    redis_set_store(object, store);
    Ok(Value::Int(len))
}

pub(super) fn redis_list_pop(
    object: &ObjectRef,
    values: &[Value],
    left: bool,
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::listPop", values.len(), 1, 2)?;
    let mut store = redis_store(object);
    let key = redis_key(&values[0])?;
    let mut elements = redis_array_property(store.get(&key))
        .iter()
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    let value = if elements.is_empty() {
        Value::Bool(false)
    } else if left {
        elements.remove(0)
    } else {
        elements.pop().unwrap_or(Value::Bool(false))
    };
    store.insert(key, Value::packed_array(elements));
    redis_set_store(object, store);
    Ok(value)
}

pub(super) fn redis_list_len(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::lLen", values.len(), 1, 1)?;
    let store = redis_store(object);
    Ok(Value::Int(
        redis_array_property(store.get(&redis_key(&values[0])?)).len() as i64,
    ))
}

pub(super) fn redis_sadd(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::sAdd", values.len(), 2, usize::MAX)?;
    let mut store = redis_store(object);
    let key = redis_key(&values[0])?;
    let mut set = redis_array_property(store.get(&key));
    let mut added = 0i64;
    for member in &values[1..] {
        let member_key = redis_key(member)?;
        if set.get(&member_key).is_none() {
            added += 1;
        }
        set.insert(member_key, member.clone());
    }
    store.insert(key, Value::Array(set));
    redis_set_store(object, store);
    Ok(Value::Int(added))
}

pub(super) fn redis_smembers(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::sMembers", values.len(), 1, 1)?;
    let store = redis_store(object);
    let set = redis_array_property(store.get(&redis_key(&values[0])?));
    Ok(Value::packed_array(
        set.iter().map(|(_, value)| value.clone()).collect(),
    ))
}

pub(super) fn redis_sismember(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::sIsMember", values.len(), 2, 2)?;
    let store = redis_store(object);
    let set = redis_array_property(store.get(&redis_key(&values[0])?));
    Ok(Value::Bool(set.get(&redis_key(&values[1])?).is_some()))
}

pub(super) fn redis_srem(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::sRem", values.len(), 2, usize::MAX)?;
    let mut store = redis_store(object);
    let key = redis_key(&values[0])?;
    let mut set = redis_array_property(store.get(&key));
    let mut removed = 0i64;
    for member in &values[1..] {
        if set.remove(&redis_key(member)?).is_some() {
            removed += 1;
        }
    }
    store.insert(key, Value::Array(set));
    redis_set_store(object, store);
    Ok(Value::Int(removed))
}

pub(super) fn redis_zadd(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::zAdd", values.len(), 3, usize::MAX)?;
    let mut store = redis_store(object);
    let key = redis_key(&values[0])?;
    let mut set = redis_array_property(store.get(&key));
    let mut added = 0i64;
    for pair in values[1..].chunks(2) {
        if pair.len() != 2 {
            return Err(
                "E_PHP_VM_REDIS_ARG_COUNT: Redis::zAdd expects score/member pairs".to_owned(),
            );
        }
        let member_key = redis_key(&pair[1])?;
        if set.get(&member_key).is_none() {
            added += 1;
        }
        set.insert(member_key, pair[1].clone());
    }
    store.insert(key, Value::Array(set));
    redis_set_store(object, store);
    Ok(Value::Int(added))
}

pub(super) fn redis_zrange(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::zRange", values.len(), 3, 5)?;
    let store = redis_store(object);
    let set = redis_array_property(store.get(&redis_key(&values[0])?));
    let entries = set
        .iter()
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    let start = to_int(&values[1])?.max(0) as usize;
    let end = to_int(&values[2])?;
    let end = if end < 0 {
        entries.len()
    } else {
        (end as usize).saturating_add(1).min(entries.len())
    };
    let slice = if start >= end || start >= entries.len() {
        Vec::new()
    } else {
        entries[start..end].to_vec()
    };
    Ok(Value::packed_array(slice))
}

pub(super) fn redis_scan(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::scan", values.len(), 1, 4)?;
    let store = redis_store(object);
    Ok(Value::packed_array(
        store
            .iter()
            .map(|(key, _)| match key {
                ArrayKey::Int(value) => Value::Int(value),
                ArrayKey::String(value) => Value::String(value.clone()),
            })
            .collect(),
    ))
}

pub(super) fn redis_set_option(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::setOption", values.len(), 2, 2)?;
    let mut options = match object.get_property(REDIS_OPTIONS_PROPERTY) {
        Some(Value::Array(array)) => array,
        _ => PhpArray::new(),
    };
    options.insert(redis_key(&values[0])?, values[1].clone());
    object.set_property(REDIS_OPTIONS_PROPERTY, Value::Array(options));
    Ok(Value::Bool(true))
}

pub(super) fn redis_get_option(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_redis_arg_count("Redis::getOption", values.len(), 1, 1)?;
    let options = match object.get_property(REDIS_OPTIONS_PROPERTY) {
        Some(Value::Array(array)) => array,
        _ => PhpArray::new(),
    };
    Ok(options
        .get(&redis_key(&values[0])?)
        .cloned()
        .unwrap_or(Value::Null))
}
