//! Runtime shim for the built-in `Redis` class, extracted from the VM module.

use super::prelude::*;
use php_runtime::api::{
    igbinary_serialize_value, igbinary_unserialize_value, msgpack_pack_value, msgpack_unpack_value,
};
use std::fmt;

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
const REDIS_MODE_PROPERTY: &str = "__redis_mode";
const REDIS_MODE_ATOMIC: i64 = 0;
const REDIS_MODE_MULTI: i64 = 1;
const REDIS_MODE_PIPELINE: i64 = 2;
const REDIS_OPT_SERIALIZER: i64 = 1;
const REDIS_SERIALIZER_NONE: i64 = 0;
const REDIS_SERIALIZER_PHP: i64 = 1;
const REDIS_SERIALIZER_IGBINARY: i64 = 2;
const REDIS_SERIALIZER_MSGPACK: i64 = 3;

#[derive(Default)]
pub(super) struct RedisClientState {
    connections: HashMap<u64, redis::Connection>,
    last_errors: HashMap<u64, String>,
}

impl fmt::Debug for RedisClientState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RedisClientState")
            .field("connections", &self.connections.keys().collect::<Vec<_>>())
            .field("last_errors", &self.last_errors)
            .finish()
    }
}

impl RedisClientState {
    fn connect(&mut self, object: &ObjectRef, host: &str, port: i64, timeout: Duration) -> bool {
        let url = format!("redis://{host}:{port}/");
        let result = redis::Client::open(url.as_str())
            .and_then(|client| client.get_connection_with_timeout(timeout))
            .and_then(|connection| {
                connection.set_read_timeout(Some(timeout))?;
                connection.set_write_timeout(Some(timeout))?;
                Ok(connection)
            });
        match result {
            Ok(connection) => {
                self.connections.insert(object.id(), connection);
                self.last_errors.remove(&object.id());
                object.set_property(REDIS_CONNECTED_PROPERTY, Value::Bool(true));
                true
            }
            Err(error) => {
                self.connections.remove(&object.id());
                self.last_errors.insert(object.id(), error.to_string());
                object.set_property(REDIS_CONNECTED_PROPERTY, Value::Bool(false));
                false
            }
        }
    }

    fn disconnect(&mut self, object: &ObjectRef) {
        self.connections.remove(&object.id());
        object.set_property(REDIS_CONNECTED_PROPERTY, Value::Bool(false));
    }

    fn is_connected(&self, object: &ObjectRef) -> bool {
        self.connections.contains_key(&object.id())
    }

    fn connection_mut(&mut self, object: &ObjectRef) -> Option<&mut redis::Connection> {
        self.connections.get_mut(&object.id())
    }
}

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
        name: "redis".to_owned().into(),
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
    object.set_property(REDIS_MODE_PROPERTY, Value::Int(REDIS_MODE_ATOMIC));
}

pub(super) fn redis_class_constant_value(class_name: &str, constant: &str) -> Option<Value> {
    if !is_redis_runtime_class(class_name) {
        return None;
    }
    let value = match constant.to_ascii_uppercase().as_str() {
        "OPT_SERIALIZER" => REDIS_OPT_SERIALIZER,
        "OPT_PREFIX" => 2,
        "OPT_READ_TIMEOUT" => 3,
        "OPT_SCAN" => 4,
        "OPT_TCP_KEEPALIVE" => 6,
        "OPT_COMPRESSION" => 7,
        "OPT_REPLY_LITERAL" => 8,
        "OPT_COMPRESSION_LEVEL" => 9,
        "OPT_NULL_MULTIBULK_AS_NULL" => 10,
        "OPT_MAX_RETRIES" => 11,
        "OPT_BACKOFF_ALGORITHM" => 12,
        "OPT_BACKOFF_BASE" => 13,
        "OPT_BACKOFF_CAP" => 14,
        "OPT_PACK_IGNORE_NUMBERS" => 15,
        "SERIALIZER_NONE" => REDIS_SERIALIZER_NONE,
        "SERIALIZER_PHP" => REDIS_SERIALIZER_PHP,
        "SERIALIZER_IGBINARY" => REDIS_SERIALIZER_IGBINARY,
        "SERIALIZER_MSGPACK" => REDIS_SERIALIZER_MSGPACK,
        "SERIALIZER_JSON" => 4,
        "COMPRESSION_NONE" => 0,
        "COMPRESSION_LZF" => 1,
        "COMPRESSION_ZSTD" => 2,
        "COMPRESSION_LZ4" => 3,
        "SCAN_NORETRY" => 0,
        "SCAN_RETRY" => 1,
        "SCAN_PREFIX" => 2,
        "SCAN_NOPREFIX" => 3,
        "ATOMIC" => REDIS_MODE_ATOMIC,
        "MULTI" => REDIS_MODE_MULTI,
        "PIPELINE" => REDIS_MODE_PIPELINE,
        _ => return None,
    };
    Some(Value::Int(value))
}

pub(super) fn call_redis_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    state: &mut RedisClientState,
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
            let host = php_string_to_lossy_string(&to_string(&values[0])?);
            let port = values.get(1).map(to_int).transpose()?.unwrap_or(6379);
            let timeout = redis_connect_timeout(values.get(2))?;
            Ok(Value::Bool(state.connect(object, &host, port, timeout)))
        }
        "close" => {
            validate_redis_arg_count("Redis::close", values.len(), 0, 0)?;
            state.disconnect(object);
            Ok(Value::Bool(true))
        }
        "ping" => {
            validate_redis_arg_count("Redis::ping", values.len(), 0, 1)?;
            redis_query_simple(state, object, "PING", &[])
        }
        "getmode" => {
            validate_redis_arg_count("Redis::getMode", values.len(), 0, 0)?;
            Ok(object
                .get_property(REDIS_MODE_PROPERTY)
                .unwrap_or(Value::Int(REDIS_MODE_ATOMIC)))
        }
        "isconnected" | "is_connected" => {
            validate_redis_arg_count("Redis::isConnected", values.len(), 0, 0)?;
            Ok(Value::Bool(state.is_connected(object)))
        }
        "auth" => {
            validate_redis_arg_count("Redis::auth", values.len(), 1, 1)?;
            redis_query_bool(state, object, "AUTH", &[redis_value_bytes(&values[0])?])
        }
        "select" => {
            validate_redis_arg_count("Redis::select", values.len(), 1, 1)?;
            let db = to_int(&values[0])?;
            let result = redis_query_bool(state, object, "SELECT", &[db.to_string().into_bytes()])?;
            if result == Value::Bool(true) {
                object.set_property(REDIS_DB_PROPERTY, Value::Int(db));
            }
            Ok(result)
        }
        "set" => redis_set(state, object, &values),
        "setex" => redis_setex(state, object, &values),
        "setnx" => redis_setnx(state, object, &values),
        "get" => redis_get(state, object, &values),
        "mget" | "getmultiple" => redis_mget(state, object, &values),
        "mset" => redis_mset(state, object, &values),
        "del" | "delete" | "unlink" => redis_del(state, object, &values),
        "exists" => redis_exists(state, object, &values),
        "expire" | "pexpire" | "persist" => redis_key_bool_result(state, object, &values, &method),
        "ttl" | "pttl" => redis_ttl(state, object, &values, &method),
        "incr" => redis_counter(state, object, &values, 1),
        "incrby" => redis_counter_by(state, object, &values, 1),
        "decr" => redis_counter(state, object, &values, -1),
        "decrby" => redis_counter_by(state, object, &values, -1),
        "hset" => redis_hset(state, object, &values),
        "hget" => redis_hget(state, object, &values),
        "hgetall" => redis_hgetall(object, &values),
        "hdel" => redis_hdel(object, &values),
        "hexists" => redis_hexists(object, &values),
        "lpush" => redis_list_push(state, object, &values, true),
        "rpush" => redis_list_push(state, object, &values, false),
        "lrange" => redis_lrange(state, object, &values),
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
            let mode = if method == "pipeline" {
                REDIS_MODE_PIPELINE
            } else {
                REDIS_MODE_MULTI
            };
            object.set_property(REDIS_MODE_PROPERTY, Value::Int(mode));
            Ok(Value::Object(object.clone()))
        }
        "exec" => {
            validate_redis_arg_count("Redis::exec", values.len(), 0, 0)?;
            object.set_property(REDIS_MODE_PROPERTY, Value::Int(REDIS_MODE_ATOMIC));
            Ok(Value::Array(PhpArray::new()))
        }
        "discard" => {
            validate_redis_arg_count("Redis::discard", values.len(), 0, 0)?;
            object.set_property(REDIS_MODE_PROPERTY, Value::Int(REDIS_MODE_ATOMIC));
            Ok(Value::Bool(true))
        }
        "scan" => redis_scan(object, &values),
        "setoption" => redis_set_option(object, &values),
        "getoption" => redis_get_option(object, &values),
        other => Err(format!(
            "E_PHP_VM_REDIS_METHOD_GAP: method Redis::{other} is not implemented by the endpoint-backed Redis client"
        )),
    }
}

pub(super) fn php_string_to_lossy_string(value: &PhpString) -> String {
    String::from_utf8_lossy(value.as_bytes()).into_owned()
}

fn redis_connect_timeout(value: Option<&Value>) -> Result<Duration, String> {
    let seconds = value.map(to_float).transpose()?.unwrap_or(1.0);
    let millis = if seconds <= 0.0 {
        100
    } else {
        (seconds * 1000.0).clamp(1.0, 60_000.0) as u64
    };
    Ok(Duration::from_millis(millis))
}

pub(super) fn redis_value_bytes(value: &Value) -> Result<Vec<u8>, String> {
    Ok(to_string(value)?.as_bytes().to_vec())
}

fn redis_serializer(object: &ObjectRef) -> i64 {
    let options = match object.get_property(REDIS_OPTIONS_PROPERTY) {
        Some(Value::Array(array)) => array,
        _ => return REDIS_SERIALIZER_NONE,
    };
    let key = ArrayKey::String(PhpString::from(REDIS_OPT_SERIALIZER.to_string().as_str()));
    options
        .get(&key)
        .and_then(|value| to_int(value).ok())
        .unwrap_or(REDIS_SERIALIZER_NONE)
}

fn redis_unsupported_serializer(serializer: i64) -> String {
    format!(
        "E_PHP_VM_REDIS_SERIALIZER_GAP: Redis serializer {serializer} is not implemented for endpoint-backed value payloads"
    )
}

fn redis_encode_cache_value(object: &ObjectRef, value: &Value) -> Result<Vec<u8>, String> {
    match redis_serializer(object) {
        REDIS_SERIALIZER_NONE => redis_value_bytes(value),
        REDIS_SERIALIZER_PHP => serialize_value(value)
            .map(|encoded| encoded.as_bytes().to_vec())
            .map_err(|error| {
                format!(
                    "E_PHP_VM_REDIS_SERIALIZE: failed to PHP-serialize Redis value: {}",
                    error.message()
                )
            }),
        REDIS_SERIALIZER_IGBINARY => igbinary_serialize_value(value)
            .map(|encoded| encoded.as_bytes().to_vec())
            .map_err(|message| {
                format!("E_PHP_VM_REDIS_SERIALIZE: failed to igbinary-serialize Redis value: {message}")
            }),
        REDIS_SERIALIZER_MSGPACK => msgpack_pack_value(value)
            .map(|encoded| encoded.as_bytes().to_vec())
            .map_err(|message| {
                format!("E_PHP_VM_REDIS_SERIALIZE: failed to MessagePack-serialize Redis value: {message}")
            }),
        other => Err(redis_unsupported_serializer(other)),
    }
}

fn redis_decode_cache_bytes(object: &ObjectRef, bytes: Vec<u8>) -> Result<Value, String> {
    let input = PhpString::from_bytes(bytes);
    match redis_serializer(object) {
        REDIS_SERIALIZER_NONE => Ok(Value::String(input)),
        REDIS_SERIALIZER_PHP => unserialize_value(&input, UnserializeOptions::default()).map_err(
            |error| {
                format!(
                    "E_PHP_VM_REDIS_UNSERIALIZE: failed to PHP-unserialize Redis value: {}",
                    error.message()
                )
            },
        ),
        REDIS_SERIALIZER_IGBINARY => igbinary_unserialize_value(&input).map_err(|message| {
            format!("E_PHP_VM_REDIS_UNSERIALIZE: failed to igbinary-unserialize Redis value: {message}")
        }),
        REDIS_SERIALIZER_MSGPACK => msgpack_unpack_value(&input).map_err(|message| {
            format!("E_PHP_VM_REDIS_UNSERIALIZE: failed to MessagePack-unserialize Redis value: {message}")
        }),
        other => Err(redis_unsupported_serializer(other)),
    }
}

fn redis_cache_value_to_php(object: &ObjectRef, value: redis::Value) -> Result<Value, String> {
    match value {
        redis::Value::Nil => Ok(Value::Bool(false)),
        redis::Value::BulkString(bytes) => redis_decode_cache_bytes(object, bytes),
        redis::Value::Array(values) => values
            .into_iter()
            .map(|value| redis_cache_value_to_php(object, value))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::packed_array),
        other => Ok(redis_value_to_php(other)),
    }
}

fn redis_command_response(
    state: &mut RedisClientState,
    object: &ObjectRef,
    command: &str,
    args: &[Vec<u8>],
) -> Result<Option<redis::Value>, String> {
    let Some(connection) = state.connection_mut(object) else {
        return Ok(None);
    };
    let mut cmd = redis::cmd(command);
    for arg in args {
        cmd.arg(arg);
    }
    match cmd.query::<redis::Value>(connection) {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            state.last_errors.insert(object.id(), error.to_string());
            state.disconnect(object);
            Ok(None)
        }
    }
}

fn redis_value_to_php(value: redis::Value) -> Value {
    match value {
        redis::Value::Nil => Value::Bool(false),
        redis::Value::Int(value) => Value::Int(value),
        redis::Value::BulkString(bytes) => Value::string(bytes),
        redis::Value::Array(values) => {
            Value::packed_array(values.into_iter().map(redis_value_to_php).collect())
        }
        redis::Value::SimpleString(value) => Value::string(value.into_bytes()),
        redis::Value::Okay => Value::Bool(true),
        redis::Value::Map(values) => {
            let mut array = PhpArray::new();
            for (key, value) in values {
                array.insert(redis_value_to_array_key(key), redis_value_to_php(value));
            }
            Value::Array(array)
        }
        redis::Value::Attribute { data, .. } => redis_value_to_php(*data),
        redis::Value::Set(values) => {
            Value::packed_array(values.into_iter().map(redis_value_to_php).collect())
        }
        redis::Value::Double(value) => Value::float(value),
        redis::Value::Boolean(value) => Value::Bool(value),
        redis::Value::VerbatimString { text, .. } => Value::string(text.into_bytes()),
        redis::Value::BigNumber(value) => Value::string(format!("{value:?}").into_bytes()),
        redis::Value::Push { data, .. } => {
            Value::packed_array(data.into_iter().map(redis_value_to_php).collect())
        }
        redis::Value::ServerError(error) => Value::string(error.to_string().into_bytes()),
        _ => Value::Bool(false),
    }
}

fn redis_value_to_array_key(value: redis::Value) -> ArrayKey {
    match value {
        redis::Value::Int(value) => ArrayKey::Int(value),
        redis::Value::BulkString(bytes) => ArrayKey::String(PhpString::from_bytes(bytes)),
        redis::Value::SimpleString(value) => ArrayKey::String(PhpString::from(value.as_str())),
        other => {
            let value = redis_value_to_php(other);
            let key = to_string_php(&value)
                .map(|string| php_string_to_lossy_string(&string))
                .unwrap_or_else(|_| value_type_name(&value).to_owned());
            ArrayKey::String(PhpString::from(key.as_str()))
        }
    }
}

fn redis_query_bool(
    state: &mut RedisClientState,
    object: &ObjectRef,
    command: &str,
    args: &[Vec<u8>],
) -> Result<Value, String> {
    let Some(response) = redis_command_response(state, object, command, args)? else {
        return Ok(Value::Bool(false));
    };
    Ok(match response {
        redis::Value::Okay | redis::Value::SimpleString(_) => Value::Bool(true),
        redis::Value::Int(value) => Value::Bool(value != 0),
        redis::Value::Boolean(value) => Value::Bool(value),
        redis::Value::Nil => Value::Bool(false),
        other => redis_value_to_php(other),
    })
}

fn redis_query_simple(
    state: &mut RedisClientState,
    object: &ObjectRef,
    command: &str,
    args: &[Vec<u8>],
) -> Result<Value, String> {
    let Some(response) = redis_command_response(state, object, command, args)? else {
        return Ok(Value::Bool(false));
    };
    Ok(match response {
        redis::Value::Okay => Value::Bool(true),
        redis::Value::SimpleString(value) => {
            if value.starts_with('+') {
                Value::string(value.into_bytes())
            } else {
                Value::string(format!("+{value}").into_bytes())
            }
        }
        other => redis_value_to_php(other),
    })
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

pub(super) fn redis_set(
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::set", values.len(), 2, 5)?;
    redis_query_bool(
        state,
        object,
        "SET",
        &[
            redis_value_bytes(&values[0])?,
            redis_encode_cache_value(object, &values[1])?,
        ],
    )
}

pub(super) fn redis_setex(
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::setex", values.len(), 3, 3)?;
    redis_query_bool(
        state,
        object,
        "SETEX",
        &[
            redis_value_bytes(&values[0])?,
            to_int(&values[1])?.to_string().into_bytes(),
            redis_encode_cache_value(object, &values[2])?,
        ],
    )
}

pub(super) fn redis_setnx(
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::setnx", values.len(), 2, 2)?;
    redis_query_bool(
        state,
        object,
        "SETNX",
        &[
            redis_value_bytes(&values[0])?,
            redis_encode_cache_value(object, &values[1])?,
        ],
    )
}

pub(super) fn redis_get(
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::get", values.len(), 1, 1)?;
    Ok(
        redis_command_response(state, object, "GET", &[redis_value_bytes(&values[0])?])?
            .map(|value| redis_cache_value_to_php(object, value))
            .transpose()?
            .unwrap_or(Value::Bool(false)),
    )
}

pub(super) fn redis_mget(
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::mget", values.len(), 1, 1)?;
    let keys = redis_array_value_entries(&values[0], "Redis::mget")?;
    let args = keys
        .into_iter()
        .map(|(_, key)| redis_value_bytes(&key))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(redis_command_response(state, object, "MGET", &args)?
        .map(|value| redis_cache_value_to_php(object, value))
        .transpose()?
        .unwrap_or(Value::Bool(false)))
}

pub(super) fn redis_mset(
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::mset", values.len(), 1, 1)?;
    let mut args = Vec::new();
    for (key, value) in redis_array_value_entries(&values[0], "Redis::mset")? {
        let key = match key {
            ArrayKey::Int(index) => index.to_string().into_bytes(),
            ArrayKey::String(name) => name.as_bytes().to_vec(),
        };
        args.push(key);
        args.push(redis_encode_cache_value(object, &value)?);
    }
    redis_query_bool(state, object, "MSET", &args)
}

pub(super) fn redis_del(
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
) -> Result<Value, String> {
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
    let args = keys
        .into_iter()
        .map(|key| redis_value_bytes(&key))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(redis_command_response(state, object, "DEL", &args)?
        .map(redis_value_to_php)
        .unwrap_or(Value::Bool(false)))
}

pub(super) fn redis_exists(
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::exists", values.len(), 1, usize::MAX)?;
    let args = values
        .iter()
        .map(redis_value_bytes)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(redis_command_response(state, object, "EXISTS", &args)?
        .map(redis_value_to_php)
        .unwrap_or(Value::Bool(false)))
}

pub(super) fn redis_key_bool_result(
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
    function: &str,
) -> Result<Value, String> {
    validate_redis_arg_count(function, values.len(), 1, 3)?;
    let command = match function {
        "persist" => "PERSIST",
        "pexpire" => "PEXPIRE",
        _ => "EXPIRE",
    };
    let mut args = vec![redis_value_bytes(&values[0])?];
    if command != "PERSIST" {
        args.push(
            values
                .get(1)
                .map(to_int)
                .transpose()?
                .unwrap_or(0)
                .to_string()
                .into_bytes(),
        );
    }
    redis_query_bool(state, object, command, &args)
}

pub(super) fn redis_ttl(
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
    function: &str,
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::ttl", values.len(), 1, 1)?;
    let command = if function == "pttl" { "PTTL" } else { "TTL" };
    Ok(
        redis_command_response(state, object, command, &[redis_value_bytes(&values[0])?])?
            .map(redis_value_to_php)
            .unwrap_or(Value::Bool(false)),
    )
}

pub(super) fn redis_counter(
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
    delta: i64,
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::counter", values.len(), 1, 1)?;
    let command = if delta >= 0 { "INCR" } else { "DECR" };
    Ok(
        redis_command_response(state, object, command, &[redis_value_bytes(&values[0])?])?
            .map(redis_value_to_php)
            .unwrap_or(Value::Bool(false)),
    )
}

pub(super) fn redis_counter_by(
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
    direction: i64,
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::counterBy", values.len(), 2, 2)?;
    let command = if direction >= 0 { "INCRBY" } else { "DECRBY" };
    redis_command_response(
        state,
        object,
        command,
        &[
            redis_value_bytes(&values[0])?,
            to_int(&values[1])?.to_string().into_bytes(),
        ],
    )
    .map(|value| value.map(redis_value_to_php).unwrap_or(Value::Bool(false)))
}

pub(super) fn redis_hset(
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::hSet", values.len(), 3, 3)?;
    Ok(redis_command_response(
        state,
        object,
        "HSET",
        &[
            redis_value_bytes(&values[0])?,
            redis_value_bytes(&values[1])?,
            redis_encode_cache_value(object, &values[2])?,
        ],
    )?
    .map(redis_value_to_php)
    .unwrap_or(Value::Bool(false)))
}

pub(super) fn redis_hget(
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::hGet", values.len(), 2, 2)?;
    Ok(redis_command_response(
        state,
        object,
        "HGET",
        &[
            redis_value_bytes(&values[0])?,
            redis_value_bytes(&values[1])?,
        ],
    )?
    .map(|value| redis_cache_value_to_php(object, value))
    .transpose()?
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
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
    left: bool,
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::listPush", values.len(), 2, usize::MAX)?;
    let command = if left { "LPUSH" } else { "RPUSH" };
    let mut args = vec![redis_value_bytes(&values[0])?];
    for value in &values[1..] {
        args.push(redis_encode_cache_value(object, value)?);
    }
    Ok(redis_command_response(state, object, command, &args)?
        .map(redis_value_to_php)
        .unwrap_or(Value::Bool(false)))
}

pub(super) fn redis_lrange(
    state: &mut RedisClientState,
    object: &ObjectRef,
    values: &[Value],
) -> Result<Value, String> {
    validate_redis_arg_count("Redis::lRange", values.len(), 3, 3)?;
    Ok(redis_command_response(
        state,
        object,
        "LRANGE",
        &[
            redis_value_bytes(&values[0])?,
            to_int(&values[1])?.to_string().into_bytes(),
            to_int(&values[2])?.to_string().into_bytes(),
        ],
    )?
    .map(|value| redis_cache_value_to_php(object, value))
    .transpose()?
    .unwrap_or(Value::Bool(false)))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn object_with_serializer(serializer: i64) -> ObjectRef {
        let object = new_redis_object("Redis", Vec::new()).unwrap();
        let mut options = PhpArray::new();
        options.insert(
            redis_key(&Value::Int(REDIS_OPT_SERIALIZER)).unwrap(),
            Value::Int(serializer),
        );
        object.set_property(REDIS_OPTIONS_PROPERTY, Value::Array(options));
        object
    }

    fn structured_payload() -> Value {
        Value::packed_array(vec![
            Value::Int(1),
            Value::string("two"),
            Value::packed_array(vec![Value::Bool(false), Value::Null]),
        ])
    }

    #[test]
    fn redis_msgpack_serializer_roundtrips_structured_payloads() {
        let object = object_with_serializer(REDIS_SERIALIZER_MSGPACK);
        let payload = structured_payload();

        let encoded = redis_encode_cache_value(&object, &payload).unwrap();

        assert_ne!(encoded, b"Array".to_vec());
        assert_eq!(redis_decode_cache_bytes(&object, encoded).unwrap(), payload);
    }

    #[test]
    fn redis_igbinary_serializer_roundtrips_structured_payloads() {
        let object = object_with_serializer(REDIS_SERIALIZER_IGBINARY);
        let payload = structured_payload();

        let encoded = redis_encode_cache_value(&object, &payload).unwrap();

        assert_ne!(encoded, b"Array".to_vec());
        assert_eq!(redis_decode_cache_bytes(&object, encoded).unwrap(), payload);
    }
}
