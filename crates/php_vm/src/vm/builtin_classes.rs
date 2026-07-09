//! Runtime implementations of built-in / optional-extension classes (std_class,
//! hash_context, PhpToken, DateTime family, SQLite, PDO, mysqli, Memcached, Imagick,
//! XSL, Phar, GD, ZipArchive, FFI, XML, Normalizer, Intl), extracted from the VM module.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::result_large_err)]

use super::prelude::*;
use std::io::Seek;

pub(super) fn is_std_class_runtime_class(class_name: &str) -> bool {
    class_name
        .trim_start_matches('\\')
        .rsplit('\\')
        .next()
        .unwrap_or(class_name)
        .eq_ignore_ascii_case("stdclass")
}

pub(super) fn is_hash_context_runtime_class(class_name: &str) -> bool {
    normalize_class_name(class_name) == "hashcontext"
}

pub(super) fn internal_hash_context_instanceof(
    object_class: &str,
    target_class: &str,
) -> Option<bool> {
    is_hash_context_runtime_class(object_class).then(|| is_hash_context_runtime_class(target_class))
}

pub(super) fn is_php_token_runtime_class(class_name: &str) -> bool {
    normalize_class_name(class_name) == "phptoken"
}

pub(super) fn internal_php_token_instanceof(
    object_class: &str,
    target_class: &str,
) -> Option<bool> {
    is_php_token_runtime_class(object_class).then(|| {
        is_php_token_runtime_class(target_class)
            || normalize_class_name(target_class) == "stringable"
    })
}

pub(super) struct PhpTokenStaticMethodValue {
    pub(super) value: Value,
    pub(super) diagnostics: Vec<php_runtime::RuntimeDiagnostic>,
}

pub(super) fn php_token_static_method_value_with_diagnostics(
    class_name: &str,
    method: &str,
    args: Vec<CallArgument>,
) -> Result<PhpTokenStaticMethodValue, String> {
    match normalize_method_name(method).as_str() {
        "tokenize" => {
            let result = php_tokenize_static_call_with_diagnostics(class_name, args)?;
            Ok(PhpTokenStaticMethodValue {
                value: Value::packed_array(
                    result
                        .tokens
                        .into_iter()
                        .map(|token| Value::Object(php_token_object(token)))
                        .collect(),
                ),
                diagnostics: result.diagnostics,
            })
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
        )),
    }
}

pub(super) fn php_token_static_method_value_for_class_with_diagnostics(
    class_name: &str,
    method: &str,
    args: Vec<CallArgument>,
    runtime_class: &RuntimeClassEntry,
    display_name: String,
) -> Result<PhpTokenStaticMethodValue, String> {
    match normalize_method_name(method).as_str() {
        "tokenize" => {
            let result = php_tokenize_static_call_with_diagnostics(class_name, args)?;
            Ok(PhpTokenStaticMethodValue {
                value: Value::packed_array(
                    result
                        .tokens
                        .into_iter()
                        .map(|token| {
                            Value::Object(php_token_object_for_class(
                                token,
                                runtime_class,
                                display_name.clone(),
                            ))
                        })
                        .collect(),
                ),
                diagnostics: result.diagnostics,
            })
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
        )),
    }
}

pub(super) fn php_tokenize_static_call_with_diagnostics(
    class_name: &str,
    args: Vec<CallArgument>,
) -> Result<php_runtime::tokenizer::TokenizeResult, String> {
    let values = call_args_to_positional("PhpToken::tokenize", args)?;
    if values.is_empty() || values.len() > 2 {
        return Err(format!(
            "E_PHP_VM_TOKENIZER_ARITY: {class_name}::tokenize expects 1 or 2 argument(s), {} given",
            values.len()
        ));
    }
    let source = to_string(&values[0])?.to_string_lossy();
    let flags = values.get(1).map(to_int).transpose()?.unwrap_or(0);
    php_runtime::tokenizer::tokenize_with_diagnostics(&source, flags)
        .map_err(|error| error.display_message())
}

pub(super) fn php_token_method_value(
    object: &ObjectRef,
    method: &str,
    args: Vec<Value>,
) -> Result<Value, String> {
    match normalize_method_name(method).as_str() {
        "gettokenname" => {
            if !args.is_empty() {
                return Err(format!(
                    "E_PHP_VM_TOKENIZER_ARITY: PhpToken::getTokenName expects 0 argument(s), {} given",
                    args.len()
                ));
            }
            Ok(php_token_name_value(object).unwrap_or(Value::Null))
        }
        "isignorable" => {
            if !args.is_empty() {
                return Err(format!(
                    "E_PHP_VM_TOKENIZER_ARITY: PhpToken::isIgnorable expects 0 argument(s), {} given",
                    args.len()
                ));
            }
            Ok(Value::Bool(
                object
                    .get_property("id")
                    .and_then(|value| match value {
                        Value::Int(id) => Some(php_runtime::tokenizer::is_ignorable_id(id)),
                        _ => None,
                    })
                    .unwrap_or(false),
            ))
        }
        "is" => {
            if args.len() != 1 {
                return Err(format!(
                    "E_PHP_VM_TOKENIZER_ARITY: PhpToken::is expects 1 argument(s), {} given",
                    args.len()
                ));
            }
            Ok(Value::Bool(php_token_matches_kind(object, &args[0])?))
        }
        "__tostring" => {
            if !args.is_empty() {
                return Err(format!(
                    "E_PHP_VM_TOKENIZER_ARITY: PhpToken::__toString expects 0 argument(s), {} given",
                    args.len()
                ));
            }
            Ok(object
                .get_property("text")
                .unwrap_or_else(|| Value::string(Vec::new())))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
            object.class_name(),
            method
        )),
    }
}

pub(super) fn php_token_matches_kind(object: &ObjectRef, kind: &Value) -> Result<bool, String> {
    match kind {
        Value::Reference(cell) => php_token_matches_kind(object, &cell.get()),
        Value::Int(value) => Ok(php_token_required_id(object)? == *value),
        Value::String(value) => {
            let candidate = value.to_string_lossy();
            let text = php_token_required_text(object)?.to_string_lossy();
            let name = php_token_name_value(object)
                .and_then(|value| match value {
                    Value::String(name) => Some(name.to_string_lossy()),
                    _ => None,
                })
                .unwrap_or_default();
            Ok(candidate == text || candidate.eq_ignore_ascii_case(&name))
        }
        Value::Array(array) => {
            for (_, value) in array.iter() {
                let matches = match value {
                    Value::Reference(cell) => php_token_matches_kind(object, &cell.get())?,
                    Value::Int(expected) => php_token_required_id(object)? == *expected,
                    Value::String(expected) => {
                        php_token_required_text(object)?.as_bytes() == expected.as_bytes()
                    }
                    other => {
                        return Err(format!(
                            "E_PHP_VM_TOKENIZER_KIND_ELEMENT_TYPE: PhpToken::is(): Argument #1 ($kind) must only have elements of type string|int, {} given",
                            type_error_value_name(other)
                        ));
                    }
                };
                if matches {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        other => Err(format!(
            "E_PHP_VM_TOKENIZER_KIND_TYPE: PhpToken::is(): Argument #1 ($kind) must be of type string|int|array, {} given",
            type_error_value_name(other)
        )),
    }
}

pub(super) fn php_token_required_id(object: &ObjectRef) -> Result<i64, String> {
    match object.get_property("id") {
        Some(Value::Int(id)) => Ok(id),
        _ => Err(
            "E_PHP_VM_TOKENIZER_UNINITIALIZED_PROPERTY: Typed property PhpToken::$id must not be accessed before initialization"
                .to_owned(),
        ),
    }
}

pub(super) fn php_token_required_text(object: &ObjectRef) -> Result<PhpString, String> {
    match object.get_property("text") {
        Some(Value::String(text)) => Ok(text),
        _ => Err(
            "E_PHP_VM_TOKENIZER_UNINITIALIZED_PROPERTY: Typed property PhpToken::$text must not be accessed before initialization"
                .to_owned(),
        ),
    }
}

pub(super) fn php_token_name_value(object: &ObjectRef) -> Option<Value> {
    let id = match object.get_property("id") {
        Some(Value::Int(id)) => id,
        _ => return None,
    };
    if let Some(name) = php_runtime::tokenizer::token_name_for_id(id) {
        return Some(Value::String(PhpString::from_test_str(name)));
    }
    if (0..=u8::MAX as i64).contains(&id)
        && let Some(Value::String(text)) = object.get_property("text")
    {
        return Some(Value::String(text));
    }
    None
}

pub(super) fn php_token_object(token: php_runtime::tokenizer::TokenizerToken) -> ObjectRef {
    let class = php_token_class();
    php_token_object_for_class(token, &class, "PhpToken")
}

pub(super) fn php_token_object_for_class(
    token: php_runtime::tokenizer::TokenizerToken,
    class: &RuntimeClassEntry,
    display_name: impl Into<String>,
) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(class, display_name);
    object.set_property("id", Value::Int(token.id));
    object.set_property("text", Value::string(token.text.into_bytes()));
    object.set_property("line", Value::Int(i64::from(token.line)));
    object.set_property("pos", Value::Int(i64::from(token.pos)));
    object
}

pub(super) fn new_php_token_object(args: Vec<CallArgument>) -> Result<ObjectRef, String> {
    let values = call_args_to_positional("PhpToken::__construct", args)?;
    if values.len() < 2 || values.len() > 4 {
        return Err(format!(
            "E_PHP_VM_TOKENIZER_ARITY: PhpToken::__construct expects 2 to 4 argument(s), {} given",
            values.len()
        ));
    }
    let id = to_int(&values[0])?;
    let text = to_string(&values[1])?;
    let line = values.get(2).map(to_int).transpose()?.unwrap_or(-1);
    let pos = values.get(3).map(to_int).transpose()?.unwrap_or(-1);
    let object = ObjectRef::new_with_display_name(&php_token_class(), "PhpToken");
    object.set_property("id", Value::Int(id));
    object.set_property("text", Value::String(text));
    object.set_property("line", Value::Int(line));
    object.set_property("pos", Value::Int(pos));
    Ok(object)
}

pub(super) fn php_token_property(
    name: &str,
    default: Value,
    type_: RuntimeType,
) -> RuntimeClassPropertyEntry {
    RuntimeClassPropertyEntry {
        name: name.to_owned(),
        default,
        type_: Some(type_),
        flags: RuntimeClassPropertyFlags {
            is_typed: true,
            ..RuntimeClassPropertyFlags::default()
        },
        hooks: RuntimeClassPropertyHooks::default(),
        attributes: Vec::new(),
    }
}

pub(super) fn php_token_properties() -> Vec<RuntimeClassPropertyEntry> {
    vec![
        php_token_property("id", Value::Int(0), RuntimeType::Int),
        php_token_property("text", Value::string(Vec::new()), RuntimeType::String),
        php_token_property("line", Value::Int(-1), RuntimeType::Int),
        php_token_property("pos", Value::Int(-1), RuntimeType::Int),
    ]
}

pub(super) fn php_token_class() -> RuntimeClassEntry {
    RuntimeClassEntry {
        name: normalize_class_name("PhpToken"),
        parent: None,
        interfaces: vec![normalize_class_name("Stringable")],
        methods: Vec::new(),
        properties: php_token_properties(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: RuntimeClassFlags::default(),
    }
}

pub(super) fn is_date_time_runtime_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "datetime" | "datetimeimmutable" | "datetimezone" | "dateinterval"
    )
}

pub(super) fn internal_date_time_class_entry(normalized: &str) -> php_ir::module::ClassEntry {
    let normalized = normalize_class_name(normalized);
    let display_name = match normalized.as_str() {
        "datetimeinterface" => "DateTimeInterface",
        "datetime" => "DateTime",
        "datetimeimmutable" => "DateTimeImmutable",
        "datetimezone" => "DateTimeZone",
        "dateinterval" => "DateInterval",
        _ => "DateTime",
    };
    let mut entry =
        empty_internal_class_entry(display_name, None, None, normalized == "datetimeinterface");
    if matches!(normalized.as_str(), "datetime" | "datetimeimmutable") {
        entry
            .interfaces
            .push(normalize_class_name("DateTimeInterface"));
    }
    entry
}

pub(super) fn internal_date_time_instanceof(
    object_class: &str,
    target_class: &str,
) -> Option<bool> {
    if !is_date_time_runtime_class(object_class) {
        return None;
    }
    let object_class = normalize_class_name(object_class);
    let target_class = normalize_class_name(target_class);
    Some(match target_class.as_str() {
        "datetimeinterface" => matches!(object_class.as_str(), "datetime" | "datetimeimmutable"),
        "datetime" => object_class == "datetime",
        "datetimeimmutable" => object_class == "datetimeimmutable",
        "datetimezone" => object_class == "datetimezone",
        "dateinterval" => object_class == "dateinterval",
        _ => false,
    })
}

pub(super) fn new_date_time_object(
    class_name: &str,
    args: Vec<CallArgument>,
    default_timezone: &str,
) -> Result<ObjectRef, String> {
    let function = format!("{}::__construct", date_time_display_name(class_name));
    let values = call_args_to_positional(&function, args)?;
    match normalize_class_name(class_name).as_str() {
        "datetime" | "datetimeimmutable" => {
            validate_date_time_arg_count(&function, values.len(), 0, 2)?;
            let text = values
                .first()
                .map(to_string)
                .transpose()?
                .map(|value| value.to_string_lossy())
                .unwrap_or_else(|| "now".to_owned());
            let timezone = values
                .get(1)
                .map(date_time_timezone_name_from_value)
                .transpose()?
                .unwrap_or_else(|| default_timezone.to_owned());
            let base = php_runtime::datetime::current_timestamp();
            let timestamp =
                php_runtime::datetime::parse_datetime_text_in_timezone(&text, base, &timezone)
                    .ok_or_else(|| {
                        format!("E_PHP_VM_DATETIME_PARSE: could not parse DateTime text {text:?}")
                    })?;
            let value = if normalize_class_name(class_name) == "datetimeimmutable" {
                php_runtime::datetime::datetime_immutable_object(timestamp, &timezone)
            } else {
                php_runtime::datetime::datetime_object(timestamp, &timezone)
            };
            date_time_object_from_value(value)
        }
        "datetimezone" => {
            validate_date_time_arg_count(&function, values.len(), 1, 1)?;
            let timezone = to_string(&values[0])?.to_string_lossy();
            php_runtime::datetime::datetimezone_object(&timezone)
                .ok_or_else(|| {
                    format!("E_PHP_VM_DATETIMEZONE_INVALID: timezone {timezone:?} is unsupported")
                })
                .and_then(date_time_object_from_value)
        }
        "dateinterval" => {
            validate_date_time_arg_count(&function, values.len(), 1, 1)?;
            let spec = to_string(&values[0])?.to_string_lossy();
            let seconds = php_runtime::datetime::parse_interval_spec(&spec).ok_or_else(|| {
                format!("E_PHP_VM_DATEINTERVAL_PARSE: interval spec {spec:?} is unsupported")
            })?;
            date_time_object_from_value(php_runtime::datetime::dateinterval_object(seconds))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"
        )),
    }
}

pub(super) fn call_date_time_method(
    object: ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
) -> Result<Value, String> {
    let class_name = object.class_name();
    let function = format!("{}::{}", date_time_display_name(&class_name), method);
    let values = call_args_to_positional(&function, args)?;
    match normalize_class_name(&class_name).as_str() {
        "datetime" | "datetimeimmutable" => {
            let immutable = normalize_class_name(&class_name) == "datetimeimmutable";
            call_date_time_like_method(object, method, values, immutable)
        }
        "datetimezone" => call_date_timezone_method(object, method, values),
        "dateinterval" => call_date_interval_method(object, method, values),
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
        )),
    }
}

pub(super) fn is_sqlite_runtime_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "sqlite3" | "sqlite3result" | "sqlite3stmt"
    )
}

pub(super) fn internal_sqlite_instanceof(object_class: &str, target_class: &str) -> Option<bool> {
    if !is_sqlite_runtime_class(object_class) {
        return None;
    }
    Some(normalize_class_name(object_class) == normalize_class_name(target_class))
}

pub(super) fn is_pdo_runtime_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "pdo" | "pdostatement" | "pdorow"
    )
}

pub(super) fn internal_pdo_instanceof(object_class: &str, target_class: &str) -> Option<bool> {
    if !is_pdo_runtime_class(object_class) {
        return None;
    }
    Some(normalize_class_name(object_class) == normalize_class_name(target_class))
}

pub(super) fn is_mysqli_runtime_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "mysqli" | "mysqli_result" | "mysqli_stmt" | "mysqli_driver" | "mysqli_warning"
    )
}

pub(super) fn internal_mysqli_instanceof(object_class: &str, target_class: &str) -> Option<bool> {
    if !is_mysqli_runtime_class(object_class) {
        return None;
    }
    Some(normalize_class_name(object_class) == normalize_class_name(target_class))
}

pub(super) fn is_memcached_runtime_class(class_name: &str) -> bool {
    normalize_class_name(class_name) == "memcached"
}

pub(super) fn is_soap_runtime_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "soapparam"
            | "soapheader"
            | "soapvar"
            | "soapfault"
            | "soapclient"
            | "soapserver"
            | "soap\\soapparam"
            | "soap\\soapheader"
            | "soap\\soapvar"
            | "soap\\soapfault"
            | "soap\\soapclient"
            | "soap\\soapserver"
            | "soap\\sdl"
            | "soap\\url"
    )
}

pub(super) fn internal_soap_instanceof(object_class: &str, target_class: &str) -> Option<bool> {
    if !is_soap_runtime_class(object_class) {
        return None;
    }
    Some(normalize_class_name(object_class) == normalize_class_name(target_class))
}

pub(super) fn is_imagick_runtime_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "imagick" | "imagickdraw" | "imagickpixel" | "imagickpixeliterator"
    )
}

pub(super) fn new_imagick_object(
    class_name: &str,
    _args: Vec<CallArgument>,
) -> Result<ObjectRef, String> {
    if !is_imagick_runtime_class(class_name) {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"
        ));
    }
    Err(format!(
        "E_PHP_VM_UNSUPPORTED_IMAGICK: class {class_name} requires an ImageMagick backend capability gate"
    ))
}

pub(super) fn call_imagick_method(
    object: &ObjectRef,
    method: &str,
    _args: Vec<CallArgument>,
) -> Result<Value, String> {
    Err(format!(
        "E_PHP_VM_UNSUPPORTED_IMAGICK: method {}::{method} requires an ImageMagick backend capability gate",
        object.display_name()
    ))
}

pub(super) fn is_xsl_runtime_class(class_name: &str) -> bool {
    normalize_class_name(class_name) == "xsltprocessor"
}

pub(super) fn new_xsl_object(
    class_name: &str,
    _args: Vec<CallArgument>,
) -> Result<ObjectRef, String> {
    if !is_xsl_runtime_class(class_name) {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"
        ));
    }
    Err(format!(
        "E_PHP_VM_UNSUPPORTED_XSL: class {class_name} requires a libxslt backend capability gate"
    ))
}

pub(super) fn call_xsl_method(
    object: &ObjectRef,
    method: &str,
    _args: Vec<CallArgument>,
) -> Result<Value, String> {
    Err(format!(
        "E_PHP_VM_UNSUPPORTED_XSL: method {}::{method} requires a libxslt backend capability gate",
        object.display_name()
    ))
}

pub(super) fn internal_memcached_instanceof(
    object_class: &str,
    target_class: &str,
) -> Option<bool> {
    if !is_memcached_runtime_class(object_class) {
        return None;
    }
    Some(normalize_class_name(target_class) == "memcached")
}

pub(super) const MEMCACHED_RES_SUCCESS: i64 = 0;
pub(super) const MEMCACHED_RES_FAILURE: i64 = 1;
pub(super) const MEMCACHED_RES_NOTFOUND: i64 = 16;
pub(super) const MEMCACHED_STORE_PROPERTY: &str = "__memcached_store";
pub(super) const MEMCACHED_SERVERS_PROPERTY: &str = "__memcached_servers";
pub(super) const MEMCACHED_OPTIONS_PROPERTY: &str = "__memcached_options";
pub(super) const MEMCACHED_RESULT_CODE_PROPERTY: &str = "__memcached_result_code";

pub(super) fn new_soap_object(
    class_name: &str,
    args: Vec<CallArgument>,
) -> Result<ObjectRef, String> {
    if !is_soap_runtime_class(class_name) {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"
        ));
    }
    let normalized = normalize_class_name(class_name);
    let display_name = soap_display_name(&normalized);
    let object = ObjectRef::new_with_display_name(&soap_runtime_class(display_name), display_name);
    call_soap_method(&object, "__construct", args)?;
    Ok(object)
}

pub(super) fn soap_runtime_class(name: &str) -> RuntimeClassEntry {
    RuntimeClassEntry {
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
        flags: RuntimeClassFlags::default(),
    }
}

pub(super) fn call_soap_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
) -> Result<Value, String> {
    let method = normalize_method_name(method);
    let class_name = normalize_class_name(&object.class_name());
    match class_name.as_str() {
        "soapparam" | "soap\\soapparam" => call_soap_param_method(object, &method, args),
        "soapheader" | "soap\\soapheader" => call_soap_header_method(object, &method, args),
        "soapvar" | "soap\\soapvar" => call_soap_var_method(object, &method, args),
        "soapfault" | "soap\\soapfault" => call_soap_fault_method(object, &method, args),
        "soapclient" | "soap\\soapclient" => call_soap_client_method(object, &method, args),
        "soapserver" | "soap\\soapserver" => call_soap_server_method(object, &method, args),
        "soap\\sdl" | "soap\\url" => Err(format!(
            "E_PHP_VM_UNSUPPORTED_SOAP: class {} is an internal SOAP helper and cannot be constructed directly",
            object.display_name()
        )),
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {}::{method} is not declared",
            object.display_name()
        )),
    }
}

fn call_soap_param_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
) -> Result<Value, String> {
    let values = call_args_to_positional(&format!("{}::{method}", object.display_name()), args)?;
    match method {
        "__construct" => {
            validate_arg_count(
                &format!("{}::__construct", object.display_name()),
                values.len(),
                2,
                2,
            )?;
            object.set_property("param_data", values[0].clone());
            object.set_property("param_name", Value::String(to_string(&values[1])?));
            Ok(Value::Null)
        }
        _ => unknown_soap_method(object, method),
    }
}

fn call_soap_header_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
) -> Result<Value, String> {
    let values = call_args_to_positional(&format!("{}::{method}", object.display_name()), args)?;
    match method {
        "__construct" => {
            validate_arg_count(
                &format!("{}::__construct", object.display_name()),
                values.len(),
                2,
                5,
            )?;
            object.set_property("namespace", Value::String(to_string(&values[0])?));
            object.set_property("name", Value::String(to_string(&values[1])?));
            object.set_property("data", values.get(2).cloned().unwrap_or(Value::Null));
            object.set_property(
                "mustUnderstand",
                values
                    .get(3)
                    .map(to_bool)
                    .transpose()?
                    .map(Value::Bool)
                    .unwrap_or(Value::Bool(false)),
            );
            object.set_property(
                "actor",
                values
                    .get(4)
                    .map(soap_string_int_or_null)
                    .transpose()?
                    .unwrap_or(Value::Null),
            );
            Ok(Value::Null)
        }
        _ => unknown_soap_method(object, method),
    }
}

fn call_soap_var_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
) -> Result<Value, String> {
    let values = call_args_to_positional(&format!("{}::{method}", object.display_name()), args)?;
    match method {
        "__construct" => {
            validate_arg_count(
                &format!("{}::__construct", object.display_name()),
                values.len(),
                2,
                6,
            )?;
            object.set_property("enc_value", values[0].clone());
            object.set_property("enc_type", soap_nullable_int(&values[1])?);
            object.set_property("enc_stype", soap_nullable_string(values.get(2))?);
            object.set_property("enc_ns", soap_nullable_string(values.get(3))?);
            object.set_property("enc_name", soap_nullable_string(values.get(4))?);
            object.set_property("enc_namens", soap_nullable_string(values.get(5))?);
            Ok(Value::Null)
        }
        _ => unknown_soap_method(object, method),
    }
}

fn call_soap_fault_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
) -> Result<Value, String> {
    let values = call_args_to_positional(&format!("{}::{method}", object.display_name()), args)?;
    match method {
        "__construct" => {
            validate_arg_count(
                &format!("{}::__construct", object.display_name()),
                values.len(),
                2,
                7,
            )?;
            let (faultcodens, faultcode) = soap_fault_code(&values[0])?;
            let message = Value::String(to_string(&values[1])?);
            object.set_property("message", message.clone());
            object.set_property("code", Value::Int(0));
            object.set_property("faultstring", message);
            object.set_property("faultcode", faultcode);
            object.set_property("faultcodens", faultcodens);
            object.set_property("faultactor", soap_nullable_string(values.get(2))?);
            object.set_property("detail", values.get(3).cloned().unwrap_or(Value::Null));
            object.set_property("_name", soap_nullable_string(values.get(4))?);
            object.set_property("headerfault", values.get(5).cloned().unwrap_or(Value::Null));
            object.set_property(
                "lang",
                values
                    .get(6)
                    .map(to_string)
                    .transpose()?
                    .map(Value::String)
                    .unwrap_or_else(|| Value::string("")),
            );
            Ok(Value::Null)
        }
        "__tostring" => {
            validate_arg_count(
                &format!("{}::__toString", object.display_name()),
                values.len(),
                0,
                0,
            )?;
            Ok(Value::string(soap_fault_string(object).into_bytes()))
        }
        _ => unknown_soap_method(object, method),
    }
}

fn call_soap_client_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
) -> Result<Value, String> {
    let values = call_args_to_positional(&format!("{}::{method}", object.display_name()), args)?;
    match method {
        "__construct" => {
            validate_arg_count(
                &format!("{}::__construct", object.display_name()),
                values.len(),
                1,
                2,
            )?;
            object.set_property("__soap_wsdl", values[0].clone());
            object.set_property(
                "__soap_options",
                values
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| Value::Array(PhpArray::new())),
            );
            object.set_property("__last_request", Value::Null);
            object.set_property("__last_response", Value::Null);
            object.set_property("__last_request_headers", Value::Null);
            object.set_property("__last_response_headers", Value::Null);
            object.set_property("__cookies", Value::Array(PhpArray::new()));
            Ok(Value::Null)
        }
        "__getlastrequest" => Ok(object.get_property("__last_request").unwrap_or(Value::Null)),
        "__getlastresponse" => Ok(object
            .get_property("__last_response")
            .unwrap_or(Value::Null)),
        "__getlastrequestheaders" => Ok(object
            .get_property("__last_request_headers")
            .unwrap_or(Value::Null)),
        "__getlastresponseheaders" => Ok(object
            .get_property("__last_response_headers")
            .unwrap_or(Value::Null)),
        "__getcookies" => Ok(object
            .get_property("__cookies")
            .unwrap_or_else(|| Value::Array(PhpArray::new()))),
        "__getfunctions" | "__gettypes" => Ok(Value::Null),
        "__setcookie" => {
            validate_arg_count(
                &format!("{}::__setCookie", object.display_name()),
                values.len(),
                1,
                2,
            )?;
            let name = to_string(&values[0])?.to_string_lossy();
            let value = values
                .get(1)
                .map(to_string)
                .transpose()?
                .map(Value::String)
                .unwrap_or(Value::Null);
            let mut cookies = match object.get_property("__cookies") {
                Some(Value::Array(array)) => array,
                _ => PhpArray::new(),
            };
            cookies.insert(ArrayKey::String(PhpString::from(name.as_str())), value);
            object.set_property("__cookies", Value::Array(cookies));
            Ok(Value::Null)
        }
        "__setlocation" => {
            validate_arg_count(
                &format!("{}::__setLocation", object.display_name()),
                values.len(),
                0,
                1,
            )?;
            let previous = object.get_property("location").unwrap_or(Value::Null);
            object.set_property("location", soap_nullable_string(values.first())?);
            Ok(previous)
        }
        "__setsoapheaders" => {
            validate_arg_count(
                &format!("{}::__setSoapHeaders", object.display_name()),
                values.len(),
                0,
                1,
            )?;
            object.set_property(
                "__default_headers",
                values.first().cloned().unwrap_or(Value::Null),
            );
            Ok(Value::Bool(true))
        }
        "__call" | "__soapcall" | "__dorequest" => Err(format!(
            "E_PHP_VM_UNSUPPORTED_SOAP: method {}::{method} requires WSDL, XML serialization, and HTTP transport support",
            object.display_name()
        )),
        _ => unknown_soap_method(object, method),
    }
}

fn call_soap_server_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
) -> Result<Value, String> {
    let values = call_args_to_positional(&format!("{}::{method}", object.display_name()), args)?;
    match method {
        "__construct" => {
            validate_arg_count(
                &format!("{}::__construct", object.display_name()),
                values.len(),
                1,
                2,
            )?;
            object.set_property("__soap_wsdl", values[0].clone());
            object.set_property(
                "__soap_options",
                values
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| Value::Array(PhpArray::new())),
            );
            object.set_property("__functions", Value::Array(PhpArray::new()));
            object.set_property("__headers", Value::Array(PhpArray::new()));
            object.set_property("__last_response", Value::Null);
            Ok(Value::Null)
        }
        "getfunctions" => Ok(object
            .get_property("__functions")
            .unwrap_or_else(|| Value::Array(PhpArray::new()))),
        "__getlastresponse" => Ok(object
            .get_property("__last_response")
            .unwrap_or(Value::Null)),
        "addfunction" => {
            validate_arg_count(
                &format!("{}::addFunction", object.display_name()),
                values.len(),
                1,
                1,
            )?;
            object.set_property("__functions", values[0].clone());
            Ok(Value::Null)
        }
        "addsoapheader" => {
            validate_arg_count(
                &format!("{}::addSoapHeader", object.display_name()),
                values.len(),
                1,
                1,
            )?;
            let mut headers = match object.get_property("__headers") {
                Some(Value::Array(array)) => array,
                _ => PhpArray::new(),
            };
            headers.append(values[0].clone());
            object.set_property("__headers", Value::Array(headers));
            Ok(Value::Null)
        }
        "setpersistence" | "setclass" | "setobject" => Ok(Value::Null),
        "fault" | "handle" => Err(format!(
            "E_PHP_VM_UNSUPPORTED_SOAP: method {}::{method} requires local SOAP server dispatch support",
            object.display_name()
        )),
        _ => unknown_soap_method(object, method),
    }
}

pub(super) fn soap_display_name(normalized: &str) -> &'static str {
    match normalized {
        "soapparam" | "soap\\soapparam" => "SoapParam",
        "soapheader" | "soap\\soapheader" => "SoapHeader",
        "soapvar" | "soap\\soapvar" => "SoapVar",
        "soapfault" | "soap\\soapfault" => "SoapFault",
        "soapclient" | "soap\\soapclient" => "SoapClient",
        "soapserver" | "soap\\soapserver" => "SoapServer",
        "soap\\sdl" => "Soap\\Sdl",
        "soap\\url" => "Soap\\Url",
        _ => "SoapFault",
    }
}

fn soap_nullable_string(value: Option<&Value>) -> Result<Value, String> {
    match value {
        Some(Value::Null) | None => Ok(Value::Null),
        Some(value) => Ok(Value::String(to_string(value)?)),
    }
}

fn soap_nullable_int(value: &Value) -> Result<Value, String> {
    match value {
        Value::Null => Ok(Value::Null),
        value => Ok(Value::Int(to_int(value)?)),
    }
}

fn soap_string_int_or_null(value: &Value) -> Result<Value, String> {
    match value {
        Value::Null => Ok(Value::Null),
        Value::Int(_) => Ok(value.clone()),
        value => Ok(Value::String(to_string(value)?)),
    }
}

fn soap_fault_code(value: &Value) -> Result<(Value, Value), String> {
    match value {
        Value::Null => Ok((Value::Null, Value::Null)),
        Value::Array(array) => {
            let ns = array
                .get(&ArrayKey::Int(0))
                .map(to_string)
                .transpose()?
                .map(Value::String)
                .unwrap_or(Value::Null);
            let code = array
                .get(&ArrayKey::Int(1))
                .map(to_string)
                .transpose()?
                .map(Value::String)
                .unwrap_or(Value::Null);
            Ok((ns, code))
        }
        value => Ok((
            Value::string("http://schemas.xmlsoap.org/soap/envelope/"),
            Value::String(to_string(value)?),
        )),
    }
}

fn soap_fault_string(object: &ObjectRef) -> String {
    let code = object
        .get_property("faultcode")
        .and_then(|value| to_string(&value).ok())
        .map(|value| value.to_string_lossy())
        .unwrap_or_default();
    let message = object
        .get_property("faultstring")
        .and_then(|value| to_string(&value).ok())
        .map(|value| value.to_string_lossy())
        .unwrap_or_default();
    if code.is_empty() {
        format!("SoapFault exception: {message}")
    } else {
        format!("SoapFault exception: [{code}] {message}")
    }
}

fn unknown_soap_method(object: &ObjectRef, method: &str) -> Result<Value, String> {
    Err(format!(
        "E_PHP_VM_UNKNOWN_METHOD: method {}::{method} is not declared",
        object.display_name()
    ))
}

fn validate_arg_count(function: &str, actual: usize, min: usize, max: usize) -> Result<(), String> {
    if actual < min || actual > max {
        let expected = if min == max {
            min.to_string()
        } else {
            format!("{min} to {max}")
        };
        return Err(format!(
            "E_PHP_VM_ARGUMENT_COUNT: {function} expects {expected} argument(s), {actual} given"
        ));
    }
    Ok(())
}

pub(super) fn new_memcached_object(
    class_name: &str,
    args: Vec<CallArgument>,
) -> Result<ObjectRef, String> {
    if !is_memcached_runtime_class(class_name) {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"
        ));
    }
    let values = call_args_to_positional("Memcached::__construct", args)?;
    validate_memcached_arg_count("Memcached::__construct", values.len(), 0, 1)?;
    let object = ObjectRef::new_with_display_name(&memcached_runtime_class(), "Memcached");
    memcached_reset_object(&object);
    Ok(object)
}

pub(super) fn memcached_runtime_class() -> RuntimeClassEntry {
    RuntimeClassEntry {
        name: "memcached".to_owned(),
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

pub(super) fn memcached_class_constant_value(class_name: &str, constant: &str) -> Option<Value> {
    if !is_memcached_runtime_class(class_name) {
        return None;
    }
    let value = match constant.to_ascii_uppercase().as_str() {
        "RES_SUCCESS" => MEMCACHED_RES_SUCCESS,
        "RES_FAILURE" => MEMCACHED_RES_FAILURE,
        "RES_NOTFOUND" | "RES_NOT_FOUND" => MEMCACHED_RES_NOTFOUND,
        "OPT_SERIALIZER" => -1003,
        "SERIALIZER_PHP" => 1,
        "SERIALIZER_IGBINARY" => 2,
        "SERIALIZER_JSON" => 3,
        "SERIALIZER_JSON_ARRAY" => 4,
        "SERIALIZER_MSGPACK" => 5,
        "OPT_COMPRESSION" => -1001,
        "OPT_PREFIX_KEY" => -1002,
        "OPT_LIBKETAMA_COMPATIBLE" => 16,
        "GET_PRESERVE_ORDER" => 1,
        _ => return None,
    };
    Some(Value::Int(value))
}

pub(super) fn memcached_reset_object(object: &ObjectRef) {
    object.set_property(MEMCACHED_STORE_PROPERTY, Value::Array(PhpArray::new()));
    object.set_property(MEMCACHED_SERVERS_PROPERTY, Value::Array(PhpArray::new()));
    object.set_property(MEMCACHED_OPTIONS_PROPERTY, Value::Array(PhpArray::new()));
    memcached_set_result(object, MEMCACHED_RES_SUCCESS);
}

pub(super) fn call_memcached_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
) -> Result<Value, String> {
    let method = normalize_method_name(method);
    let values = call_args_to_positional(&format!("Memcached::{method}"), args)?;
    match method.as_str() {
        "__construct" => {
            validate_memcached_arg_count("Memcached::__construct", values.len(), 0, 1)?;
            memcached_reset_object(object);
            Ok(Value::Null)
        }
        "addserver" => memcached_add_server(object, &values),
        "addservers" => memcached_add_servers(object, &values),
        "getserverlist" => {
            validate_memcached_arg_count("Memcached::getServerList", values.len(), 0, 0)?;
            memcached_success(
                object,
                object
                    .get_property(MEMCACHED_SERVERS_PROPERTY)
                    .unwrap_or_else(empty_array_value),
            )
        }
        "set" => memcached_set(object, &values, MemcachedWriteMode::Set),
        "add" => memcached_set(object, &values, MemcachedWriteMode::Add),
        "replace" => memcached_set(object, &values, MemcachedWriteMode::Replace),
        "get" => memcached_get(object, &values),
        "getmulti" => memcached_get_multi(object, &values),
        "setmulti" => memcached_set_multi(object, &values),
        "delete" => memcached_delete(object, &values),
        "deletemulti" => memcached_delete_multi(object, &values),
        "increment" => memcached_counter(object, &values, 1),
        "decrement" => memcached_counter(object, &values, -1),
        "touch" => memcached_touch(object, &values),
        "flush" => {
            validate_memcached_arg_count("Memcached::flush", values.len(), 0, 1)?;
            object.set_property(MEMCACHED_STORE_PROPERTY, Value::Array(PhpArray::new()));
            memcached_success(object, Value::Bool(true))
        }
        "setoption" => memcached_set_option(object, &values),
        "getoption" => memcached_get_option(object, &values),
        "setoptions" => memcached_set_options(object, &values),
        "getresultcode" => {
            validate_memcached_arg_count("Memcached::getResultCode", values.len(), 0, 0)?;
            Ok(object
                .get_property(MEMCACHED_RESULT_CODE_PROPERTY)
                .unwrap_or(Value::Int(MEMCACHED_RES_SUCCESS)))
        }
        "getresultmessage" => {
            validate_memcached_arg_count("Memcached::getResultMessage", values.len(), 0, 0)?;
            Ok(Value::string(memcached_result_message(
                memcached_result_code(object),
            )))
        }
        "append" => memcached_append_prepend(object, &values, false),
        "prepend" => memcached_append_prepend(object, &values, true),
        "cas" => memcached_cas(object, &values),
        "getstats" => {
            validate_memcached_arg_count("Memcached::getStats", values.len(), 0, 0)?;
            memcached_success(object, Value::Array(PhpArray::new()))
        }
        "getversion" => {
            validate_memcached_arg_count("Memcached::getVersion", values.len(), 0, 0)?;
            memcached_success(object, Value::Array(PhpArray::new()))
        }
        other => Err(format!(
            "E_PHP_VM_MEMCACHED_METHOD_GAP: method Memcached::{other} is not implemented in the deterministic Memcached fake backend"
        )),
    }
}

#[derive(Clone, Copy)]
pub(super) enum MemcachedWriteMode {
    Set,
    Add,
    Replace,
}

pub(super) fn validate_memcached_arg_count(
    function: &str,
    actual: usize,
    min: usize,
    max: usize,
) -> Result<(), String> {
    if actual < min || actual > max {
        return Err(format!(
            "E_PHP_VM_MEMCACHED_ARG_COUNT: {function} expects {min}..{max} argument(s), {actual} given"
        ));
    }
    Ok(())
}

pub(super) fn memcached_store(object: &ObjectRef) -> PhpArray {
    match object.get_property(MEMCACHED_STORE_PROPERTY) {
        Some(Value::Array(array)) => array,
        _ => PhpArray::new(),
    }
}

pub(super) fn memcached_set_store(object: &ObjectRef, store: PhpArray) {
    object.set_property(MEMCACHED_STORE_PROPERTY, Value::Array(store));
}

pub(super) fn memcached_set_result(object: &ObjectRef, code: i64) {
    object.set_property(MEMCACHED_RESULT_CODE_PROPERTY, Value::Int(code));
}

pub(super) fn memcached_result_code(object: &ObjectRef) -> i64 {
    match object.get_property(MEMCACHED_RESULT_CODE_PROPERTY) {
        Some(Value::Int(value)) => value,
        _ => MEMCACHED_RES_SUCCESS,
    }
}

pub(super) fn memcached_result_message(code: i64) -> &'static str {
    match code {
        MEMCACHED_RES_SUCCESS => "SUCCESS",
        MEMCACHED_RES_NOTFOUND => "NOT FOUND",
        _ => "FAILURE",
    }
}

pub(super) fn memcached_success(object: &ObjectRef, value: Value) -> Result<Value, String> {
    memcached_set_result(object, MEMCACHED_RES_SUCCESS);
    Ok(value)
}

pub(super) fn memcached_not_found(object: &ObjectRef, value: Value) -> Result<Value, String> {
    memcached_set_result(object, MEMCACHED_RES_NOTFOUND);
    Ok(value)
}

pub(super) fn memcached_add_server(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_memcached_arg_count("Memcached::addServer", values.len(), 2, 3)?;
    let mut servers = match object.get_property(MEMCACHED_SERVERS_PROPERTY) {
        Some(Value::Array(array)) => array,
        _ => PhpArray::new(),
    };
    servers.append(Value::packed_array(vec![
        Value::String(to_string(&values[0])?),
        Value::Int(to_int(&values[1])?),
        values.get(2).cloned().unwrap_or(Value::Int(0)),
    ]));
    object.set_property(MEMCACHED_SERVERS_PROPERTY, Value::Array(servers));
    memcached_success(object, Value::Bool(true))
}

pub(super) fn memcached_add_servers(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_memcached_arg_count("Memcached::addServers", values.len(), 1, 1)?;
    let servers = redis_array_value_entries(&values[0], "Memcached::addServers")?;
    let mut current = match object.get_property(MEMCACHED_SERVERS_PROPERTY) {
        Some(Value::Array(array)) => array,
        _ => PhpArray::new(),
    };
    for (_, server) in servers {
        current.append(server);
    }
    object.set_property(MEMCACHED_SERVERS_PROPERTY, Value::Array(current));
    memcached_success(object, Value::Bool(true))
}

pub(super) fn memcached_set(
    object: &ObjectRef,
    values: &[Value],
    mode: MemcachedWriteMode,
) -> Result<Value, String> {
    validate_memcached_arg_count("Memcached::set", values.len(), 2, 4)?;
    let mut store = memcached_store(object);
    let key = redis_key(&values[0])?;
    let exists = store.get(&key).is_some();
    let should_write = match mode {
        MemcachedWriteMode::Set => true,
        MemcachedWriteMode::Add => !exists,
        MemcachedWriteMode::Replace => exists,
    };
    if !should_write {
        return memcached_not_found(object, Value::Bool(false));
    }
    store.insert(key, values[1].clone());
    memcached_set_store(object, store);
    memcached_success(object, Value::Bool(true))
}

pub(super) fn memcached_get(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_memcached_arg_count("Memcached::get", values.len(), 1, 3)?;
    let store = memcached_store(object);
    match store.get(&redis_key(&values[0])?).cloned() {
        Some(value) => memcached_success(object, value),
        None => memcached_not_found(object, Value::Bool(false)),
    }
}

pub(super) fn memcached_get_multi(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_memcached_arg_count("Memcached::getMulti", values.len(), 1, 2)?;
    let keys = redis_array_value_entries(&values[0], "Memcached::getMulti")?;
    let store = memcached_store(object);
    let mut result = PhpArray::new();
    for (_, key_value) in keys {
        let key = redis_key(&key_value)?;
        if let Some(value) = store.get(&key).cloned() {
            result.insert(key, value);
        }
    }
    memcached_success(object, Value::Array(result))
}

pub(super) fn memcached_set_multi(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_memcached_arg_count("Memcached::setMulti", values.len(), 1, 2)?;
    let mut store = memcached_store(object);
    for (key, value) in redis_array_value_entries(&values[0], "Memcached::setMulti")? {
        let key = match key {
            ArrayKey::Int(index) => ArrayKey::String(PhpString::from(index.to_string().as_str())),
            ArrayKey::String(name) => ArrayKey::String(name),
        };
        store.insert(key, value);
    }
    memcached_set_store(object, store);
    memcached_success(object, Value::Bool(true))
}

pub(super) fn memcached_delete(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_memcached_arg_count("Memcached::delete", values.len(), 1, 2)?;
    let mut store = memcached_store(object);
    let removed = store.remove(&redis_key(&values[0])?).is_some();
    memcached_set_store(object, store);
    if removed {
        memcached_success(object, Value::Bool(true))
    } else {
        memcached_not_found(object, Value::Bool(false))
    }
}

pub(super) fn memcached_delete_multi(
    object: &ObjectRef,
    values: &[Value],
) -> Result<Value, String> {
    validate_memcached_arg_count("Memcached::deleteMulti", values.len(), 1, 2)?;
    let keys = redis_array_value_entries(&values[0], "Memcached::deleteMulti")?;
    let mut store = memcached_store(object);
    for (_, key) in keys {
        store.remove(&redis_key(&key)?);
    }
    memcached_set_store(object, store);
    memcached_success(object, Value::Bool(true))
}

pub(super) fn memcached_counter(
    object: &ObjectRef,
    values: &[Value],
    direction: i64,
) -> Result<Value, String> {
    validate_memcached_arg_count("Memcached::counter", values.len(), 1, 4)?;
    let mut store = memcached_store(object);
    let key = redis_key(&values[0])?;
    let current = store.get(&key).map(to_int).transpose()?;
    let offset = values.get(1).map(to_int).transpose()?.unwrap_or(1);
    let next = match current {
        Some(current) => current.saturating_add(offset * direction).max(0),
        None => match values.get(2).map(to_int).transpose()? {
            Some(initial) => initial,
            None => return memcached_not_found(object, Value::Bool(false)),
        },
    };
    store.insert(key, Value::Int(next));
    memcached_set_store(object, store);
    memcached_success(object, Value::Int(next))
}

pub(super) fn memcached_touch(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_memcached_arg_count("Memcached::touch", values.len(), 2, 2)?;
    let store = memcached_store(object);
    if store.get(&redis_key(&values[0])?).is_some() {
        memcached_success(object, Value::Bool(true))
    } else {
        memcached_not_found(object, Value::Bool(false))
    }
}

pub(super) fn memcached_set_option(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_memcached_arg_count("Memcached::setOption", values.len(), 2, 2)?;
    let mut options = match object.get_property(MEMCACHED_OPTIONS_PROPERTY) {
        Some(Value::Array(array)) => array,
        _ => PhpArray::new(),
    };
    options.insert(redis_key(&values[0])?, values[1].clone());
    object.set_property(MEMCACHED_OPTIONS_PROPERTY, Value::Array(options));
    memcached_success(object, Value::Bool(true))
}

pub(super) fn memcached_get_option(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_memcached_arg_count("Memcached::getOption", values.len(), 1, 1)?;
    let options = match object.get_property(MEMCACHED_OPTIONS_PROPERTY) {
        Some(Value::Array(array)) => array,
        _ => PhpArray::new(),
    };
    memcached_success(
        object,
        options
            .get(&redis_key(&values[0])?)
            .cloned()
            .unwrap_or(Value::Null),
    )
}

pub(super) fn memcached_set_options(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_memcached_arg_count("Memcached::setOptions", values.len(), 1, 1)?;
    let mut options = match object.get_property(MEMCACHED_OPTIONS_PROPERTY) {
        Some(Value::Array(array)) => array,
        _ => PhpArray::new(),
    };
    for (key, value) in redis_array_value_entries(&values[0], "Memcached::setOptions")? {
        let key = match key {
            ArrayKey::Int(index) => ArrayKey::String(PhpString::from(index.to_string().as_str())),
            ArrayKey::String(name) => ArrayKey::String(name),
        };
        options.insert(key, value);
    }
    object.set_property(MEMCACHED_OPTIONS_PROPERTY, Value::Array(options));
    memcached_success(object, Value::Bool(true))
}

pub(super) fn memcached_append_prepend(
    object: &ObjectRef,
    values: &[Value],
    prepend: bool,
) -> Result<Value, String> {
    validate_memcached_arg_count("Memcached::append", values.len(), 2, 2)?;
    let mut store = memcached_store(object);
    let key = redis_key(&values[0])?;
    let Some(current) = store.get(&key) else {
        return memcached_not_found(object, Value::Bool(false));
    };
    let current = to_string(current)?;
    let value = to_string(&values[1])?;
    let mut bytes = Vec::new();
    if prepend {
        bytes.extend_from_slice(value.as_bytes());
        bytes.extend_from_slice(current.as_bytes());
    } else {
        bytes.extend_from_slice(current.as_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }
    store.insert(key, Value::string(bytes));
    memcached_set_store(object, store);
    memcached_success(object, Value::Bool(true))
}

pub(super) fn memcached_cas(object: &ObjectRef, values: &[Value]) -> Result<Value, String> {
    validate_memcached_arg_count("Memcached::cas", values.len(), 3, 4)?;
    let mut store = memcached_store(object);
    store.insert(redis_key(&values[1])?, values[2].clone());
    memcached_set_store(object, store);
    memcached_success(object, Value::Bool(true))
}

pub(super) fn is_phar_runtime_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "phar" | "phardata" | "pharfileinfo"
    )
}

pub(super) fn internal_phar_instanceof(object_class: &str, target_class: &str) -> Option<bool> {
    if !is_phar_runtime_class(object_class) {
        return None;
    }
    Some(normalize_class_name(object_class) == normalize_class_name(target_class))
}

pub(super) fn new_phar_object(
    class_name: &str,
    args: Vec<CallArgument>,
    runtime_context: &RuntimeContext,
) -> Result<ObjectRef, String> {
    let normalized = normalize_class_name(class_name);
    match normalized.as_str() {
        "phar" => {
            let values = call_args_to_positional("Phar::__construct", args)?;
            validate_phar_arg_count("Phar::__construct", values.len(), 1, 3)?;
            let filename = to_string(&values[0])?.to_string_lossy();
            let raw = Path::new(&filename);
            let path = if raw.is_absolute() {
                raw.to_path_buf()
            } else {
                runtime_context.cwd.join(raw)
            };
            if !runtime_context.filesystem.allows_path(&path) {
                return Err(format!(
                    "E_PHP_VM_PHAR_PATH_DENIED: PHAR archive path {} is outside allowed filesystem roots",
                    path.display()
                ));
            }
            let object = ObjectRef::new_with_display_name(&phar_runtime_class("Phar"), "Phar");
            object.set_property(
                "__phar_path",
                Value::string(path.to_string_lossy().as_bytes().to_vec()),
            );
            spl_file_set_path(&object, &path.to_string_lossy());
            if path.exists() {
                let archive = php_runtime::PharArchive::open(&path)
                    .map_err(|error| format!("{}: {}", error.diagnostic_id(), error.message()))?;
                if let Some(alias) = archive.alias.as_deref() {
                    object.set_property("__phar_alias", Value::string(alias));
                }
                object.set_property("__phar_entries", Value::Int(archive.len() as i64));
                object.set_property("__phar_stub", Value::string(archive.stub));
            } else {
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|error| {
                        format!(
                            "E_PHP_RUNTIME_PHAR_OPEN: failed to create {}: {error}",
                            path.display()
                        )
                    })?;
                if let Some(alias) = values.get(2).filter(|value| !matches!(value, Value::Null)) {
                    let alias = to_string(alias)?.to_string_lossy();
                    object.set_property("__phar_alias", Value::string(alias));
                }
                object.set_property("__phar_entries", Value::Int(0));
                object.set_property("__phar_stub", Value::string(Vec::new()));
            }
            Ok(object)
        }
        "phardata" => Err(
            "E_PHP_VM_PHAR_DATA_GAP: PharData tar/zip archive objects are not implemented in the PHAR MVP"
                .to_owned(),
        ),
        "pharfileinfo" => Err(
            "E_PHP_VM_PHAR_FILEINFO_GAP: PharFileInfo objects are created by archive iteration, which is not implemented in the PHAR MVP"
                .to_owned(),
        ),
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"
        )),
    }
}

pub(super) fn phar_runtime_class(name: &str) -> RuntimeClassEntry {
    RuntimeClassEntry {
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
        flags: RuntimeClassFlags::default(),
    }
}

pub(super) fn validate_phar_arg_count(
    function: &str,
    actual: usize,
    min: usize,
    max: usize,
) -> Result<(), String> {
    if actual < min || actual > max {
        return Err(format!(
            "E_PHP_VM_PHAR_ARG_COUNT: {function} expects {min}..{max} argument(s), {actual} given"
        ));
    }
    Ok(())
}

pub(super) fn call_phar_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    runtime_context: &RuntimeContext,
) -> Result<Value, String> {
    let class_name = object.class_name();
    if normalize_class_name(&class_name) != "phar" {
        return Err(format!(
            "E_PHP_VM_PHAR_CLASS_GAP: {class_name} object methods are not implemented in the PHAR MVP"
        ));
    }
    let method = normalize_method_name(method);
    match method.as_str() {
        "getalias" => {
            validate_phar_arg_count("Phar::getAlias", args.len(), 0, 0)?;
            Ok(object
                .get_property("__phar_alias")
                .unwrap_or_else(|| Value::string(Vec::new())))
        }
        "getpath" => {
            validate_phar_arg_count("Phar::getPath", args.len(), 0, 0)?;
            Ok(object
                .get_property("__phar_path")
                .unwrap_or_else(|| Value::string(Vec::new())))
        }
        "getstub" => {
            validate_phar_arg_count("Phar::getStub", args.len(), 0, 0)?;
            Ok(object
                .get_property("__phar_stub")
                .unwrap_or_else(|| Value::string(Vec::new())))
        }
        "count" => {
            validate_phar_arg_count("Phar::count", args.len(), 0, 1)?;
            Ok(object
                .get_property("__phar_entries")
                .unwrap_or(Value::Int(0)))
        }
        "offsetexists" => {
            validate_phar_arg_count("Phar::offsetExists", args.len(), 1, 1)?;
            let entry = to_string(&args[0].value)?.to_string_lossy();
            let path = phar_object_path(object)?;
            if !runtime_context.filesystem.allows_path(&path) {
                return Err(format!(
                    "E_PHP_VM_PHAR_PATH_DENIED: PHAR archive path {} is outside allowed filesystem roots",
                    path.display()
                ));
            }
            let archive = php_runtime::PharArchive::open(&path)
                .map_err(|error| format!("{}: {}", error.diagnostic_id(), error.message()))?;
            Ok(Value::Bool(archive.entry(&entry).is_some()))
        }
        _ if spl_file_method_is_supported(&method) => {
            call_spl_file_method(object, &method, args, runtime_context)
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method Phar::{method} is not implemented"
        )),
    }
}

pub(super) fn phar_object_path(object: &ObjectRef) -> Result<PathBuf, String> {
    let Some(path) = object.get_property("__phar_path") else {
        return Err("E_PHP_VM_PHAR_STATE: Phar object is missing archive path".to_owned());
    };
    Ok(PathBuf::from(to_string(&path)?.to_string_lossy()))
}

pub(super) fn is_zip_runtime_class(class_name: &str) -> bool {
    normalize_class_name(class_name) == "ziparchive"
}

pub(super) fn internal_zip_instanceof(object_class: &str, target_class: &str) -> Option<bool> {
    if !is_zip_runtime_class(object_class) {
        return None;
    }
    Some(matches!(
        normalize_class_name(target_class).as_str(),
        "ziparchive" | "countable"
    ))
}

pub(super) fn is_gd_runtime_class(class_name: &str) -> bool {
    normalize_class_name(class_name) == "gdimage"
}

pub(super) fn is_fileinfo_runtime_class(class_name: &str) -> bool {
    normalize_class_name(class_name) == "finfo"
}

pub(super) fn internal_fileinfo_instanceof(object_class: &str, target_class: &str) -> Option<bool> {
    is_fileinfo_runtime_class(object_class).then(|| is_fileinfo_runtime_class(target_class))
}

pub(super) fn internal_gd_instanceof(object_class: &str, target_class: &str) -> Option<bool> {
    if !is_gd_runtime_class(object_class) {
        return None;
    }
    Some(normalize_class_name(target_class) == "gdimage")
}

pub(super) fn is_internal_extension_resource_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "imap\\connection"
            | "ldap\\connection"
            | "ldap\\result"
            | "ldap\\resultentry"
            | "ssh2\\channel"
            | "ssh2\\session"
            | "ssh2\\sftp"
            | "shmop"
            | "sysvmessagequeue"
            | "sysvsemaphore"
            | "sysvsharedmemory"
    )
}

pub(super) fn internal_extension_resource_instanceof(
    object_class: &str,
    target_class: &str,
) -> Option<bool> {
    if !is_internal_extension_resource_class(object_class) {
        return None;
    }
    Some(normalize_class_name(object_class) == normalize_class_name(target_class))
}

pub(super) fn new_zip_object(
    class_name: &str,
    args: Vec<CallArgument>,
    _runtime_context: &RuntimeContext,
) -> Result<ObjectRef, String> {
    if !is_zip_runtime_class(class_name) {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"
        ));
    }
    let values = call_args_to_positional("ZipArchive::__construct", args)?;
    validate_zip_arg_count("ZipArchive::__construct", values.len(), 0, 0)?;
    let object = ObjectRef::new_with_display_name(&zip_runtime_class(), "ZipArchive");
    zip_reset_object(&object);
    Ok(object)
}

pub(super) fn new_fileinfo_object(
    class_name: &str,
    args: Vec<CallArgument>,
) -> Result<ObjectRef, String> {
    if !is_fileinfo_runtime_class(class_name) {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"
        ));
    }
    let values = call_args_to_positional("finfo::__construct", args)?;
    if values.len() > 2 {
        return Err(format!(
            "E_PHP_VM_FILEINFO_ARG_COUNT: finfo::__construct expects 0 to 2 argument(s), {} given",
            values.len()
        ));
    }
    let flags = values.first().map(to_int).transpose()?.unwrap_or(0);
    let magic_file = match values.get(1) {
        Some(Value::Null | Value::Uninitialized) | None => None,
        Some(value) => Some(to_string(value)?.to_string_lossy()),
    };
    let object = ObjectRef::new_with_display_name(&fileinfo_runtime_class(), "finfo");
    object.set_property("__fileinfo_flags", Value::Int(flags));
    object.set_property(
        "__fileinfo_magic_file",
        magic_file.map(Value::string).unwrap_or(Value::Null),
    );
    Ok(object)
}

pub(super) fn fileinfo_runtime_class() -> RuntimeClassEntry {
    RuntimeClassEntry {
        name: "finfo".to_owned(),
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

pub(super) fn zip_runtime_class() -> RuntimeClassEntry {
    RuntimeClassEntry {
        name: "ziparchive".to_owned(),
        parent: None,
        interfaces: vec!["Countable".to_owned()],
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

pub(super) fn zip_class_constant_value(class_name: &str, constant: &str) -> Option<Value> {
    if !is_zip_runtime_class(class_name) {
        return None;
    }
    let value = match constant.to_ascii_uppercase().as_str() {
        "CREATE" => ZIP_CREATE,
        "EXCL" => ZIP_EXCL,
        "CHECKCONS" => ZIP_CHECKCONS,
        "OVERWRITE" => ZIP_OVERWRITE,
        "RDONLY" => ZIP_RDONLY,
        "FL_NOCASE" => ZIP_FL_NOCASE,
        "FL_NODIR" => ZIP_FL_NODIR,
        "FL_UNCHANGED" => ZIP_FL_UNCHANGED,
        "FL_OVERWRITE" => ZIP_FL_OVERWRITE,
        "FL_OPEN_FILE_NOW" => ZIP_FL_OPEN_FILE_NOW,
        "LENGTH_TO_END" => ZIP_LENGTH_TO_END,
        "CM_STORE" => ZIP_CM_STORE,
        "CM_DEFLATE" => ZIP_CM_DEFLATE,
        "CM_BZIP2" => ZIP_CM_BZIP2,
        "CM_XZ" => ZIP_CM_XZ,
        "EM_NONE" => ZIP_EM_NONE,
        "EM_TRAD_PKWARE" => ZIP_EM_TRAD_PKWARE,
        "EM_AES_128" => ZIP_EM_AES_128,
        "EM_AES_192" => ZIP_EM_AES_192,
        "EM_AES_256" => ZIP_EM_AES_256,
        "ER_OK" => 0,
        "ER_EXISTS" => ZIP_ER_EXISTS,
        "ER_RDONLY" => ZIP_ER_RDONLY,
        "AFL_RDONLY" => ZIP_AFL_RDONLY,
        "AFL_CREATE_OR_KEEP_FILE_FOR_EMPTY_ARCHIVE" => {
            ZIP_AFL_CREATE_OR_KEEP_FILE_FOR_EMPTY_ARCHIVE
        }
        _ => return None,
    };
    Some(Value::Int(value))
}

pub(super) fn new_mysqli_object(
    class_name: &str,
    args: Vec<CallArgument>,
    mysql: &mut php_runtime::MysqlState,
) -> Result<ObjectRef, String> {
    match normalize_class_name(class_name).as_str() {
        "mysqli" => {
            let values = call_args_to_positional("mysqli::__construct", args)?;
            validate_mysqli_arg_count("mysqli::__construct", values.len(), 0, 6)?;
            let object = mysqli_object(None);
            if !values.is_empty()
                && let Some(id) = mysqli_connect_from_test_dsn(mysql)
            {
                mysqli_set_connection_id(&object, id);
            }
            sync_mysqli_error_properties(&object, mysql);
            Ok(object)
        }
        "mysqli_result" => Err(
            "E_PHP_VM_MYSQLI_RESULT_CONSTRUCT: mysqli_result objects are created by mysqli::query"
                .to_owned(),
        ),
        "mysqli_stmt" => {
            let values = call_args_to_positional("mysqli_stmt::__construct", args)?;
            validate_mysqli_arg_count("mysqli_stmt::__construct", values.len(), 0, 1)?;
            let Some(Value::Object(connection)) = values.first() else {
                return Ok(mysqli_stmt_object(None));
            };
            let Some(connection_id) = mysqli_connection_id(connection) else {
                return Ok(mysqli_stmt_object(None));
            };
            match mysql.stmt_init(connection_id) {
                Ok(statement_id) => Ok(mysqli_stmt_object(Some(statement_id))),
                Err(_) => Ok(mysqli_stmt_object(None)),
            }
        }
        "mysqli_driver" | "mysqli_warning" => Ok(mysqli_empty_object(class_name)),
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"
        )),
    }
}

pub(super) fn call_mysqli_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    mysql: &mut php_runtime::MysqlState,
    compiled: &CompiledUnit,
    stack: &mut CallStack,
) -> Result<Value, String> {
    let method = normalize_method_name(method);
    match normalize_class_name(&object.class_name()).as_str() {
        "mysqli" => call_mysqli_connection_method(object, &method, args, mysql),
        "mysqli_result" => call_mysqli_result_method(object, &method, args, mysql),
        "mysqli_stmt" => call_mysqli_stmt_method(object, &method, args, mysql, compiled, stack),
        other => Err(format!(
            "E_PHP_VM_MYSQLI_METHOD_GAP: method {other}::{method} is not implemented in the mysqli MVP"
        )),
    }
}

pub(super) fn call_mysqli_connection_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    mysql: &mut php_runtime::MysqlState,
) -> Result<Value, String> {
    let values = call_args_to_positional(&format!("mysqli::{method}"), args)?;
    match method {
        "__construct" | "realconnect" | "real_connect" | "connect" => {
            validate_mysqli_arg_count("mysqli::real_connect", values.len(), 0, 7)?;
            if let Some(old_id) = mysqli_connection_id(object) {
                mysql.close(old_id);
                object.unset_property("__mysqli_connection");
            }
            if let Some(id) = mysqli_connect_from_test_dsn(mysql) {
                mysqli_set_connection_id(object, id);
                sync_mysqli_error_properties(object, mysql);
                Ok(Value::Bool(true))
            } else {
                sync_mysqli_error_properties(object, mysql);
                Ok(Value::Bool(false))
            }
        }
        "query" => {
            validate_mysqli_arg_count("mysqli::query", values.len(), 1, 2)?;
            let Some(id) = mysqli_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            let sql = to_string(&values[0])?.to_string_lossy();
            match mysql.query(id, &sql) {
                Ok(Some(result_id)) => {
                    sync_mysqli_error_properties(object, mysql);
                    let result = mysqli_result_object(result_id);
                    result.set_property("num_rows", Value::Int(mysql.num_rows(result_id)));
                    Ok(Value::Object(result))
                }
                Ok(None) => {
                    sync_mysqli_error_properties(object, mysql);
                    Ok(Value::Bool(true))
                }
                Err(_) => {
                    sync_mysqli_error_properties(object, mysql);
                    Ok(Value::Bool(false))
                }
            }
        }
        "realescapestring" | "real_escape_string" | "escape_string" => {
            validate_mysqli_arg_count("mysqli::real_escape_string", values.len(), 1, 1)?;
            let value = to_string(&values[0])?;
            Ok(Value::string(mysql_escape_string(value.as_bytes())))
        }
        "affectedrows" | "affected_rows" => {
            validate_mysqli_arg_count("mysqli::affected_rows", values.len(), 0, 0)?;
            let Some(id) = mysqli_connection_id(object) else {
                return Ok(Value::Int(-1));
            };
            Ok(Value::Int(mysql.affected_rows(id)))
        }
        "insertid" | "insert_id" => {
            validate_mysqli_arg_count("mysqli::insert_id", values.len(), 0, 0)?;
            let Some(id) = mysqli_connection_id(object) else {
                return Ok(Value::Int(0));
            };
            Ok(Value::Int(mysql.last_insert_id(id)))
        }
        "close" => {
            validate_mysqli_arg_count("mysqli::close", values.len(), 0, 0)?;
            let Some(id) = mysqli_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            object.unset_property("__mysqli_connection");
            Ok(Value::Bool(mysql.close(id)))
        }
        "selectdb" | "select_db" => {
            validate_mysqli_arg_count(&format!("mysqli::{method}"), values.len(), 1, 1)?;
            let Some(id) = mysqli_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            let database = to_string(&values[0])?.to_string_lossy();
            Ok(Value::Bool(mysql.select_db(id, &database).is_ok()))
        }
        "setcharset" | "set_charset" => {
            validate_mysqli_arg_count(&format!("mysqli::{method}"), values.len(), 1, 1)?;
            let Some(id) = mysqli_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            let charset = to_string(&values[0])?.to_string_lossy();
            Ok(Value::Bool(mysql.set_charset(id, &charset).is_ok()))
        }
        "prepare" => {
            validate_mysqli_arg_count("mysqli::prepare", values.len(), 1, 1)?;
            let Some(id) = mysqli_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            let sql = to_string(&values[0])?.to_string_lossy();
            match mysql.prepare_statement(id, &sql) {
                Ok(statement_id) => Ok(Value::Object(mysqli_stmt_object(Some(statement_id)))),
                Err(_) => {
                    sync_mysqli_error_properties(object, mysql);
                    Ok(Value::Bool(false))
                }
            }
        }
        other => Err(format!(
            "E_PHP_VM_MYSQLI_METHOD_GAP: method mysqli::{other} is not implemented in the mysqli MVP"
        )),
    }
}

pub(super) fn call_mysqli_result_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    mysql: &mut php_runtime::MysqlState,
) -> Result<Value, String> {
    let values = call_args_to_positional(&format!("mysqli_result::{method}"), args)?;
    let Some(id) = mysqli_result_id(object) else {
        return Ok(Value::Bool(false));
    };
    match method {
        "fetchassoc" | "fetch_assoc" => {
            validate_mysqli_arg_count("mysqli_result::fetch_assoc", values.len(), 0, 0)?;
            Ok(mysql.fetch_array(id, php_runtime::MYSQLI_ASSOC))
        }
        "fetchrow" | "fetch_row" => {
            validate_mysqli_arg_count("mysqli_result::fetch_row", values.len(), 0, 0)?;
            Ok(mysql.fetch_array(id, php_runtime::MYSQLI_NUM))
        }
        "fetcharray" | "fetch_array" => {
            validate_mysqli_arg_count("mysqli_result::fetch_array", values.len(), 0, 1)?;
            let mode = values
                .first()
                .map(to_int)
                .transpose()?
                .unwrap_or(php_runtime::MYSQLI_BOTH);
            Ok(mysql.fetch_array(id, mode))
        }
        "free" | "close" => {
            validate_mysqli_arg_count("mysqli_result::free", values.len(), 0, 0)?;
            object.unset_property("__mysqli_result");
            Ok(Value::Bool(mysql.free_result(id)))
        }
        other => Err(format!(
            "E_PHP_VM_MYSQLI_RESULT_METHOD_GAP: method mysqli_result::{other} is not implemented in the mysqli MVP"
        )),
    }
}

pub(super) fn call_mysqli_stmt_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    mysql: &mut php_runtime::MysqlState,
    compiled: &CompiledUnit,
    stack: &mut CallStack,
) -> Result<Value, String> {
    match method {
        "prepare" => {
            let values = call_args_to_positional("mysqli_stmt::prepare", args)?;
            validate_mysqli_arg_count("mysqli_stmt::prepare", values.len(), 1, 1)?;
            let Some(id) = mysqli_stmt_id(object) else {
                return Ok(Value::Bool(false));
            };
            let sql = to_string(&values[0])?.to_string_lossy();
            let ok = mysql.stmt_prepare(id, &sql).is_ok();
            sync_mysqli_stmt_properties(object, mysql);
            Ok(Value::Bool(ok))
        }
        "bindparam" | "bind_param" => {
            validate_mysqli_arg_count("mysqli_stmt::bind_param", args.len(), 2, usize::MAX)?;
            let values = call_args_to_positional("mysqli_stmt::bind_param", args.clone())?;
            let types = to_string(&values[0])?.to_string_lossy();
            if types.chars().count() != args.len().saturating_sub(1) {
                return Ok(Value::Bool(false));
            }
            let refs = collect_mysqli_call_refs(compiled, &args[1..], stack)?;
            set_mysqli_stmt_refs(object, "__mysqli_stmt_param_refs", refs);
            object.set_property(
                "__mysqli_stmt_param_types",
                Value::String(PhpString::from(types.into_bytes())),
            );
            Ok(Value::Bool(true))
        }
        "execute" => {
            let values = call_args_to_positional("mysqli_stmt::execute", args)?;
            validate_mysqli_arg_count("mysqli_stmt::execute", values.len(), 0, 1)?;
            let Some(id) = mysqli_stmt_id(object) else {
                return Ok(Value::Bool(false));
            };
            let params: Vec<Value> = if let Some(Value::Array(params)) = values.first() {
                params.iter().map(|(_, value)| value.clone()).collect()
            } else {
                mysqli_stmt_refs(object, "__mysqli_stmt_param_refs")
                    .into_iter()
                    .map(|cell| cell.get())
                    .collect()
            };
            let ok = mysql.stmt_execute(id, &params).is_ok();
            sync_mysqli_stmt_properties(object, mysql);
            Ok(Value::Bool(ok))
        }
        "getresult" | "get_result" => {
            let values = call_args_to_positional("mysqli_stmt::get_result", args)?;
            validate_mysqli_arg_count("mysqli_stmt::get_result", values.len(), 0, 0)?;
            let Some(id) = mysqli_stmt_id(object) else {
                return Ok(Value::Bool(false));
            };
            let Some(result_id) = mysql.stmt_result(id) else {
                return Ok(Value::Bool(false));
            };
            let result = mysqli_result_object(result_id);
            result.set_property("num_rows", Value::Int(mysql.num_rows(result_id)));
            Ok(Value::Object(result))
        }
        "bindresult" | "bind_result" => {
            validate_mysqli_arg_count("mysqli_stmt::bind_result", args.len(), 1, usize::MAX)?;
            let refs = collect_mysqli_call_refs(compiled, &args, stack)?;
            set_mysqli_stmt_refs(object, "__mysqli_stmt_result_refs", refs);
            Ok(Value::Bool(true))
        }
        "fetch" => {
            let values = call_args_to_positional("mysqli_stmt::fetch", args)?;
            validate_mysqli_arg_count("mysqli_stmt::fetch", values.len(), 0, 0)?;
            let Some(id) = mysqli_stmt_id(object) else {
                return Ok(Value::Bool(false));
            };
            let refs = mysqli_stmt_refs(object, "__mysqli_stmt_result_refs");
            let Some(row) = mysql.stmt_fetch_row(id) else {
                return Ok(Value::Bool(false));
            };
            for (cell, value) in refs.into_iter().zip(row) {
                cell.set(value);
            }
            Ok(Value::Bool(true))
        }
        "numrows" | "num_rows" => {
            let values = call_args_to_positional("mysqli_stmt::num_rows", args)?;
            validate_mysqli_arg_count("mysqli_stmt::num_rows", values.len(), 0, 0)?;
            let Some(id) = mysqli_stmt_id(object) else {
                return Ok(Value::Int(0));
            };
            Ok(Value::Int(mysql.stmt_num_rows(id)))
        }
        "affectedrows" | "affected_rows" => {
            let values = call_args_to_positional("mysqli_stmt::affected_rows", args)?;
            validate_mysqli_arg_count("mysqli_stmt::affected_rows", values.len(), 0, 0)?;
            let Some(id) = mysqli_stmt_id(object) else {
                return Ok(Value::Int(-1));
            };
            Ok(Value::Int(mysql.stmt_affected_rows(id)))
        }
        "insertid" | "insert_id" => {
            let values = call_args_to_positional("mysqli_stmt::insert_id", args)?;
            validate_mysqli_arg_count("mysqli_stmt::insert_id", values.len(), 0, 0)?;
            let Some(id) = mysqli_stmt_id(object) else {
                return Ok(Value::Int(0));
            };
            Ok(Value::Int(mysql.stmt_insert_id(id)))
        }
        "errno" => {
            let values = call_args_to_positional("mysqli_stmt::errno", args)?;
            validate_mysqli_arg_count("mysqli_stmt::errno", values.len(), 0, 0)?;
            let Some(id) = mysqli_stmt_id(object) else {
                return Ok(Value::Int(1));
            };
            Ok(Value::Int(mysql.stmt_errno(id)))
        }
        "error" => {
            let values = call_args_to_positional("mysqli_stmt::error", args)?;
            validate_mysqli_arg_count("mysqli_stmt::error", values.len(), 0, 0)?;
            let Some(id) = mysqli_stmt_id(object) else {
                return Ok(Value::string("not an open mysqli_stmt"));
            };
            Ok(Value::String(PhpString::from(
                mysql.stmt_error(id).into_bytes(),
            )))
        }
        "sqlstate" => {
            let values = call_args_to_positional("mysqli_stmt::sqlstate", args)?;
            validate_mysqli_arg_count("mysqli_stmt::sqlstate", values.len(), 0, 0)?;
            let Some(id) = mysqli_stmt_id(object) else {
                return Ok(Value::string("HY000"));
            };
            Ok(Value::String(PhpString::from(
                mysql.stmt_sqlstate(id).into_bytes(),
            )))
        }
        "free_result" | "freeresult" => {
            let values = call_args_to_positional("mysqli_stmt::free_result", args)?;
            validate_mysqli_arg_count("mysqli_stmt::free_result", values.len(), 0, 0)?;
            let Some(id) = mysqli_stmt_id(object) else {
                return Ok(Value::Bool(false));
            };
            Ok(Value::Bool(mysql.stmt_free_result(id)))
        }
        "close" => {
            let values = call_args_to_positional("mysqli_stmt::close", args)?;
            validate_mysqli_arg_count("mysqli_stmt::close", values.len(), 0, 0)?;
            let Some(id) = mysqli_stmt_id(object) else {
                return Ok(Value::Bool(false));
            };
            object.unset_property("__mysqli_stmt");
            Ok(Value::Bool(mysql.stmt_close(id)))
        }
        other => Err(format!(
            "E_PHP_VM_MYSQLI_STMT_METHOD_GAP: method mysqli_stmt::{other} is not implemented"
        )),
    }
}

pub(super) fn mysqli_connect_from_test_dsn(mysql: &mut php_runtime::MysqlState) -> Option<i64> {
    if mysqli_sqlite_compat_enabled() {
        return mysql.connect_sqlite_compat().ok();
    }
    let Some(options) = php_runtime::MysqlConnectOptions::from_test_env() else {
        mysql.record_connect_error(
            2002,
            format!(
                "live mysqli connections require {}; selected SQLite compatibility fixtures require {}=1",
                php_runtime::MYSQL_TEST_DSN_ENV,
                php_runtime::MYSQLI_SQLITE_COMPAT_ENV
            ),
        );
        return None;
    };
    match options {
        Ok(options) => mysql.connect(&options).ok(),
        Err(error) => {
            mysql.record_connect_error(2005, error.message);
            None
        }
    }
}

pub(super) fn mysqli_sqlite_compat_enabled() -> bool {
    std::env::var(php_runtime::MYSQLI_SQLITE_COMPAT_ENV).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

pub(super) fn mysqli_object(connection_id: Option<i64>) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&mysqli_runtime_class("mysqli"), "mysqli");
    if let Some(id) = connection_id {
        mysqli_set_connection_id(&object, id);
    }
    object.set_property("connect_errno", Value::Int(0));
    object.set_property("connect_error", Value::string(""));
    object.set_property("errno", Value::Int(0));
    object.set_property("error", Value::string(""));
    object.set_property("affected_rows", Value::Int(0));
    object.set_property("insert_id", Value::Int(0));
    object.set_property(
        "client_info",
        Value::string(php_runtime::MYSQLND_CLIENT_INFO),
    );
    object.set_property(
        "client_version",
        Value::Int(php_runtime::MYSQLND_CLIENT_VERSION),
    );
    object
}

pub(super) fn mysqli_empty_object(class_name: &str) -> ObjectRef {
    ObjectRef::new_with_display_name(
        &mysqli_runtime_class(class_name),
        mysqli_display_name(class_name),
    )
}

pub(super) fn mysqli_result_object(result_id: i64) -> ObjectRef {
    let object =
        ObjectRef::new_with_display_name(&mysqli_runtime_class("mysqli_result"), "mysqli_result");
    object.set_property("__mysqli_result", Value::Int(result_id));
    object.set_property("num_rows", Value::Int(0));
    object
}

pub(super) fn mysqli_stmt_object(statement_id: Option<i64>) -> ObjectRef {
    let object =
        ObjectRef::new_with_display_name(&mysqli_runtime_class("mysqli_stmt"), "mysqli_stmt");
    if let Some(id) = statement_id {
        object.set_property("__mysqli_stmt", Value::Int(id));
    }
    object.set_property("affected_rows", Value::Int(0));
    object.set_property("insert_id", Value::Int(0));
    object.set_property("num_rows", Value::Int(0));
    object.set_property("errno", Value::Int(0));
    object.set_property("error", Value::string(""));
    object.set_property("sqlstate", Value::string("00000"));
    object
}

pub(super) fn mysqli_runtime_class(name: &str) -> RuntimeClassEntry {
    RuntimeClassEntry {
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
        flags: RuntimeClassFlags::default(),
    }
}

pub(super) fn validate_zip_arg_count(
    function: &str,
    actual: usize,
    min: usize,
    max: usize,
) -> Result<(), String> {
    if actual < min || actual > max {
        return Err(format!(
            "E_PHP_VM_ZIP_ARG_COUNT: {function} expects {min}..{max} argument(s), {actual} given"
        ));
    }
    Ok(())
}

pub(super) fn mysqli_display_name(class_name: &str) -> String {
    match normalize_class_name(class_name).as_str() {
        "mysqli" => "mysqli".to_owned(),
        "mysqli_result" => "mysqli_result".to_owned(),
        "mysqli_stmt" => "mysqli_stmt".to_owned(),
        "mysqli_driver" => "mysqli_driver".to_owned(),
        "mysqli_warning" => "mysqli_warning".to_owned(),
        _ => class_name.to_owned(),
    }
}

pub(super) fn mysqli_set_connection_id(object: &ObjectRef, id: i64) {
    object.set_property("__mysqli_connection", Value::Int(id));
}

pub(super) fn mysqli_connection_id(object: &ObjectRef) -> Option<i64> {
    match object.get_property("__mysqli_connection") {
        Some(Value::Int(id)) => Some(id),
        _ => None,
    }
}

pub(super) fn mysqli_result_id(object: &ObjectRef) -> Option<i64> {
    match object.get_property("__mysqli_result") {
        Some(Value::Int(id)) => Some(id),
        _ => None,
    }
}

pub(super) fn mysqli_stmt_id(object: &ObjectRef) -> Option<i64> {
    match object.get_property("__mysqli_stmt") {
        Some(Value::Int(id)) => Some(id),
        _ => None,
    }
}

pub(super) fn set_mysqli_stmt_refs(object: &ObjectRef, property: &str, refs: Vec<ReferenceCell>) {
    let mut array = PhpArray::new();
    for cell in refs {
        array.append(Value::Reference(cell));
    }
    object.set_property(property, Value::Array(array));
}

pub(super) fn mysqli_stmt_refs(object: &ObjectRef, property: &str) -> Vec<ReferenceCell> {
    let Some(Value::Array(array)) = object.get_property(property) else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|(_, value)| match value {
            Value::Reference(cell) => Some(cell.clone()),
            _ => None,
        })
        .collect()
}

pub(super) fn collect_mysqli_call_refs(
    compiled: &CompiledUnit,
    args: &[CallArgument],
    stack: &mut CallStack,
) -> Result<Vec<ReferenceCell>, String> {
    let mut refs = Vec::with_capacity(args.len());
    for arg in args {
        let Some(cell) = call_argument_reference_cell(compiled, None, arg, stack)? else {
            return Err(
                "E_PHP_VM_MYSQLI_REFERENCE: mysqli_stmt bind arguments must be variables"
                    .to_owned(),
            );
        };
        refs.push(cell);
    }
    Ok(refs)
}

pub(super) fn sync_mysqli_stmt_properties(object: &ObjectRef, mysql: &php_runtime::MysqlState) {
    if let Some(id) = mysqli_stmt_id(object) {
        object.set_property("affected_rows", Value::Int(mysql.stmt_affected_rows(id)));
        object.set_property("insert_id", Value::Int(mysql.stmt_insert_id(id)));
        object.set_property("num_rows", Value::Int(mysql.stmt_num_rows(id)));
        object.set_property("errno", Value::Int(mysql.stmt_errno(id)));
        object.set_property("error", Value::string(mysql.stmt_error(id).into_bytes()));
        object.set_property(
            "sqlstate",
            Value::string(mysql.stmt_sqlstate(id).into_bytes()),
        );
    }
}

pub(super) fn sync_mysqli_error_properties(object: &ObjectRef, mysql: &php_runtime::MysqlState) {
    let errno =
        mysqli_connection_id(object).map_or_else(|| mysql.connect_errno(), |id| mysql.errno(id));
    let error =
        mysqli_connection_id(object).map_or_else(|| mysql.connect_error(), |id| mysql.error(id));
    object.set_property("connect_errno", Value::Int(mysql.connect_errno()));
    object.set_property(
        "connect_error",
        Value::String(PhpString::from(mysql.connect_error().into_bytes())),
    );
    object.set_property("errno", Value::Int(errno));
    object.set_property("error", Value::String(PhpString::from(error.into_bytes())));
    if let Some(id) = mysqli_connection_id(object) {
        object.set_property("affected_rows", Value::Int(mysql.affected_rows(id)));
        object.set_property("insert_id", Value::Int(mysql.last_insert_id(id)));
    }
}

pub(super) fn mysql_escape_string(value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    for byte in value {
        match byte {
            0 => out.extend_from_slice(b"\\0"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\'' => out.extend_from_slice(b"\\'"),
            b'"' => out.extend_from_slice(b"\\\""),
            0x1a => out.extend_from_slice(b"\\Z"),
            other => out.push(*other),
        }
    }
    out
}

pub(super) fn validate_mysqli_arg_count(
    function: &str,
    actual: usize,
    min: usize,
    max: usize,
) -> Result<(), String> {
    if actual < min || actual > max {
        let expected = if min == max {
            min.to_string()
        } else {
            format!("{min} to {max}")
        };
        return Err(format!(
            "E_PHP_VM_MYSQLI_ARG_COUNT: {function} expects {expected} argument(s), {actual} given"
        ));
    }
    Ok(())
}

pub(super) fn call_zip_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    runtime_context: &RuntimeContext,
) -> Result<Value, String> {
    let values = call_args_to_positional(&format!("ZipArchive::{method}"), args)?;
    match normalize_method_name(method).as_str() {
        "open" => {
            validate_zip_arg_count("ZipArchive::open", values.len(), 1, 2)?;
            let filename = to_string(&values[0])?.to_string_lossy();
            if filename.is_empty() {
                return Err(
                    "E_PHP_VM_SPL_VALUE_ERROR: ZipArchive::open(): Argument #1 ($filename) must not be empty"
                        .to_owned(),
                );
            }
            let flags = values.get(1).map(to_int).transpose()?.unwrap_or(0);
            let path = zip_resolve_path(&filename, runtime_context)?;
            let archive_flags = if flags & ZIP_RDONLY != 0 {
                ZIP_AFL_RDONLY
            } else {
                0
            };
            if flags & (ZIP_CREATE | ZIP_OVERWRITE) != 0 {
                if flags & ZIP_EXCL != 0 && path.exists() {
                    zip_reset_object(object);
                    return Ok(Value::Bool(false));
                }
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        format!(
                            "E_PHP_VM_ZIP_WRITE: failed to create archive directory `{}`: {error}",
                            parent.display()
                        )
                    })?;
                }
                object.set_property(
                    "__zip_path",
                    Value::string(path.to_string_lossy().as_bytes().to_vec()),
                );
                object.set_property(
                    "filename",
                    Value::string(path.to_string_lossy().as_bytes().to_vec()),
                );
                object.set_property("__zip_write_mode", Value::Bool(true));
                object.set_property("__zip_archive_flags", Value::Int(archive_flags));
                let (entries, comment) = if path.exists() && flags & ZIP_OVERWRITE == 0 {
                    zip_load_write_state(&path).unwrap_or_else(|_| (PhpArray::new(), Vec::new()))
                } else {
                    (PhpArray::new(), Vec::new())
                };
                let entry_count = entries.iter().len();
                object.set_property("__zip_write_entries", Value::Array(entries));
                object.set_property("__zip_comment", Value::string(comment));
                object.set_property("comment", zip_object_comment_value(object));
                object.set_property("numFiles", Value::Int(entry_count as i64));
                object.set_property("status", Value::Int(0));
                object.set_property("statusSys", Value::Int(0));
                object.set_property("lastId", Value::Int(-1));
                return Ok(Value::Bool(true));
            }
            let (entries, comment) = match zip_load_write_state(&path) {
                Ok(state) => state,
                Err(_) => {
                    zip_reset_object(object);
                    return Ok(Value::Bool(false));
                }
            };
            let entry_count = entries.iter().len();
            object.set_property(
                "__zip_path",
                Value::string(path.to_string_lossy().as_bytes().to_vec()),
            );
            object.set_property(
                "filename",
                Value::string(path.to_string_lossy().as_bytes().to_vec()),
            );
            object.set_property("__zip_write_mode", Value::Bool(true));
            object.set_property("__zip_archive_flags", Value::Int(archive_flags));
            object.set_property("__zip_write_entries", Value::Array(entries));
            object.set_property("__zip_comment", Value::string(comment));
            object.set_property("comment", zip_object_comment_value(object));
            object.set_property("numFiles", Value::Int(entry_count as i64));
            object.set_property("status", Value::Int(0));
            object.set_property("statusSys", Value::Int(0));
            object.set_property("lastId", Value::Int(-1));
            Ok(Value::Bool(true))
        }
        "close" => {
            validate_zip_arg_count("ZipArchive::close", values.len(), 0, 0)?;
            if !zip_object_write_mode(object) && zip_object_path(object).is_none() {
                return Err(
                    "E_PHP_VM_SPL_VALUE_ERROR: Invalid or uninitialized Zip object".to_owned(),
                );
            }
            if zip_object_write_mode(object) {
                let Some(path) = zip_object_path(object) else {
                    zip_reset_object(object);
                    return Ok(Value::Bool(false));
                };
                if !zip_object_readonly(object) {
                    if zip_pending_entries(object).iter().len() == 0
                        && !zip_archive_flag_enabled(
                            object,
                            ZIP_AFL_CREATE_OR_KEEP_FILE_FOR_EMPTY_ARCHIVE,
                        )
                    {
                        match fs::remove_file(&path) {
                            Ok(()) => {}
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                            Err(error) => {
                                return Err(format!(
                                    "E_PHP_VM_ZIP_WRITE: failed to remove empty zip archive `{}`: {error}",
                                    path.display()
                                ));
                            }
                        }
                    } else {
                        zip_write_archive(object, &path)?;
                    }
                }
            }
            zip_reset_object(object);
            Ok(Value::Bool(true))
        }
        "count" => {
            validate_zip_arg_count("ZipArchive::count", values.len(), 0, 0)?;
            Ok(Value::Int(zip_entry_count_for_object(object)? as i64))
        }
        "getfromname" => {
            validate_zip_arg_count("ZipArchive::getFromName", values.len(), 1, 3)?;
            let name = to_string(&values[0])?.to_string_lossy();
            let max_len = zip_optional_length(values.get(1))?;
            if zip_object_write_mode(object) {
                return match zip_pending_read_name(object, &name, max_len)? {
                    Some(bytes) => Ok(Value::string(bytes)),
                    None => Ok(Value::Bool(false)),
                };
            }
            let Some(path) = zip_object_path(object) else {
                return Ok(Value::Bool(false));
            };
            match zip_read_name(&path, &name, max_len)? {
                Some(bytes) => Ok(Value::string(bytes)),
                None => Ok(Value::Bool(false)),
            }
        }
        "getfromindex" => {
            validate_zip_arg_count("ZipArchive::getFromIndex", values.len(), 1, 3)?;
            let index = to_int(&values[0])?;
            let max_len = zip_optional_length(values.get(1))?;
            if zip_object_write_mode(object) {
                return match zip_pending_read_index(object, index, max_len)? {
                    Some(bytes) => Ok(Value::string(bytes)),
                    None => Ok(Value::Bool(false)),
                };
            }
            let Some(path) = zip_object_path(object) else {
                return Ok(Value::Bool(false));
            };
            match zip_read_index(&path, index, max_len)? {
                Some(bytes) => Ok(Value::string(bytes)),
                None => Ok(Value::Bool(false)),
            }
        }
        "locatename" => {
            validate_zip_arg_count("ZipArchive::locateName", values.len(), 1, 2)?;
            let name = to_string(&values[0])?.to_string_lossy();
            let flags = values.get(1).map(to_int).transpose()?.unwrap_or(0);
            if zip_object_write_mode(object) {
                return match zip_pending_locate_name(object, &name, flags)? {
                    Some(index) => Ok(Value::Int(index as i64)),
                    None => Ok(Value::Bool(false)),
                };
            }
            let Some(path) = zip_object_path(object) else {
                return Ok(Value::Bool(false));
            };
            match zip_locate_name(&path, &name, flags)? {
                Some(index) => Ok(Value::Int(index as i64)),
                None => Ok(Value::Bool(false)),
            }
        }
        "statindex" => {
            validate_zip_arg_count("ZipArchive::statIndex", values.len(), 1, 2)?;
            let index = to_int(&values[0])?;
            if zip_object_write_mode(object) {
                return match zip_pending_stat_index(object, index)? {
                    Some(stat) => Ok(Value::Array(stat)),
                    None => Ok(Value::Bool(false)),
                };
            }
            let Some(path) = zip_object_path(object) else {
                return Ok(Value::Bool(false));
            };
            match zip_stat_index(&path, index)? {
                Some(stat) => Ok(Value::Array(stat)),
                None => Ok(Value::Bool(false)),
            }
        }
        "statname" => {
            validate_zip_arg_count("ZipArchive::statName", values.len(), 1, 2)?;
            let name = to_string(&values[0])?.to_string_lossy();
            if zip_object_write_mode(object) {
                return match zip_pending_stat_name(object, &name)? {
                    Some(stat) => Ok(Value::Array(stat)),
                    None => Ok(Value::Bool(false)),
                };
            }
            let Some(path) = zip_object_path(object) else {
                return Ok(Value::Bool(false));
            };
            match zip_stat_name(&path, &name)? {
                Some(stat) => Ok(Value::Array(stat)),
                None => Ok(Value::Bool(false)),
            }
        }
        "getnameindex" => {
            validate_zip_arg_count("ZipArchive::getNameIndex", values.len(), 1, 2)?;
            let index = to_int(&values[0])?;
            if zip_object_write_mode(object) {
                return match zip_pending_name_index(object, index)? {
                    Some(name) => Ok(Value::string(name.into_bytes())),
                    None => Ok(Value::Bool(false)),
                };
            }
            let Some(path) = zip_object_path(object) else {
                return Ok(Value::Bool(false));
            };
            match zip_name_index(&path, index)? {
                Some(name) => Ok(Value::string(name.into_bytes())),
                None => Ok(Value::Bool(false)),
            }
        }
        "extractto" => {
            validate_zip_arg_count("ZipArchive::extractTo", values.len(), 1, 2)?;
            let destination = to_string(&values[0])?.to_string_lossy();
            let entries = zip_extract_entries(values.get(1))?;
            let Some(path) = zip_object_path(object) else {
                return Ok(Value::Bool(false));
            };
            zip_extract_to(&path, &destination, entries, runtime_context).map(Value::Bool)
        }
        "addemptydir" => {
            validate_zip_arg_count("ZipArchive::addEmptyDir", values.len(), 1, 2)?;
            if !zip_object_write_mode(object) {
                return Ok(Value::Bool(false));
            }
            if let Some(value) = zip_readonly_mutation_value(object) {
                return Ok(value);
            }
            let name = zip_normalize_entry_name(&to_string(&values[0])?.to_string_lossy(), true)?;
            let flags = values.get(1).map(to_int).transpose()?.unwrap_or(0);
            if !zip_append_write_entry(object, "dir", name, Vec::new(), flags)? {
                return Ok(Value::Bool(false));
            }
            Ok(Value::Bool(true))
        }
        "addfromstring" => {
            validate_zip_arg_count("ZipArchive::addFromString", values.len(), 2, 3)?;
            if !zip_object_write_mode(object) {
                return Ok(Value::Bool(false));
            }
            if let Some(value) = zip_readonly_mutation_value(object) {
                return Ok(value);
            }
            let name = zip_normalize_entry_name(&to_string(&values[0])?.to_string_lossy(), false)?;
            let contents = to_string(&values[1])?.as_bytes().to_vec();
            let flags = values
                .get(2)
                .map(to_int)
                .transpose()?
                .unwrap_or(ZIP_FL_OVERWRITE);
            if !zip_append_write_entry(object, "file", name, contents, flags)? {
                return Ok(Value::Bool(false));
            }
            Ok(Value::Bool(true))
        }
        "addfile" => {
            validate_zip_arg_count("ZipArchive::addFile", values.len(), 1, 5)?;
            if !zip_object_write_mode(object) {
                return Ok(Value::Bool(false));
            }
            if let Some(value) = zip_readonly_mutation_value(object) {
                return Ok(value);
            }
            let file_name = to_string(&values[0])?.to_string_lossy();
            let file_path = zip_resolve_path(&file_name, runtime_context)?;
            let mut contents = fs::read(&file_path).map_err(|error| {
                format!(
                    "E_PHP_VM_ZIP_WRITE: failed to read file `{}` for archive entry: {error}",
                    file_path.display()
                )
            })?;
            let entry_name = values
                .get(1)
                .map(to_string)
                .transpose()?
                .map(|name| name.to_string_lossy())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| {
                    file_path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| file_name.clone())
                });
            let start = values.get(2).map(to_int).transpose()?.unwrap_or(0).max(0) as usize;
            let length = values
                .get(3)
                .map(to_int)
                .transpose()?
                .unwrap_or(ZIP_LENGTH_TO_END);
            let flags = values
                .get(4)
                .map(to_int)
                .transpose()?
                .unwrap_or(ZIP_FL_OVERWRITE);
            if start >= contents.len() {
                contents.clear();
            } else {
                contents = contents[start..].to_vec();
                if length > 0 {
                    contents.truncate(length as usize);
                }
            }
            let name = zip_normalize_entry_name(&entry_name, false)?;
            if !zip_append_write_entry(object, "file", name, contents, flags)? {
                return Ok(Value::Bool(false));
            }
            Ok(Value::Bool(true))
        }
        "deleteindex" => {
            validate_zip_arg_count("ZipArchive::deleteIndex", values.len(), 1, 1)?;
            if !zip_object_write_mode(object) {
                return Ok(Value::Bool(false));
            }
            if let Some(value) = zip_readonly_mutation_value(object) {
                return Ok(value);
            }
            Ok(Value::Bool(zip_delete_pending_index(
                object,
                to_int(&values[0])?,
            )?))
        }
        "deletename" => {
            validate_zip_arg_count("ZipArchive::deleteName", values.len(), 1, 1)?;
            if !zip_object_write_mode(object) {
                return Ok(Value::Bool(false));
            }
            if let Some(value) = zip_readonly_mutation_value(object) {
                return Ok(value);
            }
            let name = zip_normalize_entry_name(&to_string(&values[0])?.to_string_lossy(), false)?;
            Ok(Value::Bool(zip_delete_pending_name(object, &name)?))
        }
        "renameindex" => {
            validate_zip_arg_count("ZipArchive::renameIndex", values.len(), 2, 2)?;
            if !zip_object_write_mode(object) {
                return Ok(Value::Bool(false));
            }
            if let Some(value) = zip_readonly_mutation_value(object) {
                return Ok(value);
            }
            let name = zip_normalize_entry_name(&to_string(&values[1])?.to_string_lossy(), false)?;
            Ok(Value::Bool(zip_rename_pending_index(
                object,
                to_int(&values[0])?,
                name,
            )?))
        }
        "renamename" => {
            validate_zip_arg_count("ZipArchive::renameName", values.len(), 2, 2)?;
            if !zip_object_write_mode(object) {
                return Ok(Value::Bool(false));
            }
            if let Some(value) = zip_readonly_mutation_value(object) {
                return Ok(value);
            }
            let source =
                zip_normalize_entry_name(&to_string(&values[0])?.to_string_lossy(), false)?;
            let target =
                zip_normalize_entry_name(&to_string(&values[1])?.to_string_lossy(), false)?;
            Ok(Value::Bool(zip_rename_pending_name(
                object, &source, target,
            )?))
        }
        "getarchivecomment" => {
            validate_zip_arg_count("ZipArchive::getArchiveComment", values.len(), 0, 1)?;
            Ok(zip_object_comment_value(object))
        }
        "setarchivecomment" => {
            validate_zip_arg_count("ZipArchive::setArchiveComment", values.len(), 1, 1)?;
            if !zip_object_write_mode(object) {
                return Ok(Value::Bool(false));
            }
            if let Some(value) = zip_readonly_mutation_value(object) {
                return Ok(value);
            }
            let comment = to_string(&values[0])?.as_bytes().to_vec();
            if comment.len() >= u16::MAX as usize {
                return Err(zip_comment_too_long_value_error(
                    "ZipArchive::setArchiveComment",
                    "#1 ($comment)",
                ));
            }
            object.set_property("__zip_comment", Value::string(comment));
            object.set_property("comment", zip_object_comment_value(object));
            object.set_property("status", Value::Int(0));
            Ok(Value::Bool(true))
        }
        "getcommentindex" => {
            validate_zip_arg_count("ZipArchive::getCommentIndex", values.len(), 1, 2)?;
            match zip_pending_comment_index(object, to_int(&values[0])?)? {
                Some(comment) => Ok(Value::string(comment)),
                None => Ok(Value::Bool(false)),
            }
        }
        "getcommentname" => {
            validate_zip_arg_count("ZipArchive::getCommentName", values.len(), 1, 2)?;
            let name = to_string(&values[0])?.to_string_lossy();
            if name.is_empty() {
                return Err(
                    "E_PHP_VM_SPL_VALUE_ERROR: ZipArchive::getCommentName(): Argument #1 ($name) must not be empty"
                        .to_owned(),
                );
            }
            match zip_pending_comment_name(object, &name)? {
                Some(comment) => Ok(Value::string(comment)),
                None => Ok(Value::Bool(false)),
            }
        }
        "setcommentindex" => {
            validate_zip_arg_count("ZipArchive::setCommentIndex", values.len(), 2, 2)?;
            if !zip_object_write_mode(object) {
                return Ok(Value::Bool(false));
            }
            if let Some(value) = zip_readonly_mutation_value(object) {
                return Ok(value);
            }
            let comment = to_string(&values[1])?.as_bytes().to_vec();
            if comment.len() >= u16::MAX as usize {
                return Err(zip_comment_too_long_value_error(
                    "ZipArchive::setCommentIndex",
                    "#2 ($comment)",
                ));
            }
            Ok(Value::Bool(zip_set_pending_comment_index(
                object,
                to_int(&values[0])?,
                comment,
            )?))
        }
        "setcommentname" => {
            validate_zip_arg_count("ZipArchive::setCommentName", values.len(), 2, 2)?;
            if !zip_object_write_mode(object) {
                return Ok(Value::Bool(false));
            }
            if let Some(value) = zip_readonly_mutation_value(object) {
                return Ok(value);
            }
            let name = zip_normalize_entry_name(&to_string(&values[0])?.to_string_lossy(), false)?;
            let comment = to_string(&values[1])?.as_bytes().to_vec();
            if comment.len() >= u16::MAX as usize {
                return Err(zip_comment_too_long_value_error(
                    "ZipArchive::setCommentName",
                    "#2 ($comment)",
                ));
            }
            Ok(Value::Bool(zip_set_pending_comment_name(
                object, &name, comment,
            )?))
        }
        "getarchiveflag" => {
            validate_zip_arg_count("ZipArchive::getArchiveFlag", values.len(), 1, 2)?;
            Ok(Value::Int(
                if zip_archive_flag_enabled(object, to_int(&values[0])?) {
                    1
                } else {
                    0
                },
            ))
        }
        "setarchiveflag" => {
            validate_zip_arg_count("ZipArchive::setArchiveFlag", values.len(), 2, 3)?;
            let flag = to_int(&values[0])?;
            let enabled = to_bool(&values[1])?;
            if zip_object_readonly(object) && flag == ZIP_AFL_RDONLY && !enabled {
                object.set_property("status", Value::Int(ZIP_ER_RDONLY));
                return Ok(Value::Bool(false));
            }
            zip_set_archive_flag(object, flag, enabled);
            object.set_property("status", Value::Int(0));
            Ok(Value::Bool(true))
        }
        _ => Err(format!(
            "E_PHP_VM_ZIP_METHOD_GAP: ZipArchive::{method} is not implemented"
        )),
    }
}

pub(super) fn zip_open_uses_empty_file(
    method: &str,
    args: &[CallArgument],
    runtime_context: &RuntimeContext,
) -> bool {
    if normalize_method_name(method) != "open"
        || args.is_empty()
        || args.len() > 2
        || args.iter().any(|arg| arg.name.is_some())
    {
        return false;
    }
    let Ok(filename) = to_string(&args[0].value) else {
        return false;
    };
    let filename = filename.to_string_lossy();
    if filename.is_empty() {
        return false;
    }
    let flags = match args.get(1).map(|arg| to_int(&arg.value)).transpose() {
        Ok(flags) => flags.unwrap_or(0),
        Err(_) => return false,
    };
    if flags & (ZIP_CREATE | ZIP_OVERWRITE) != 0 {
        return false;
    }
    let Ok(path) = zip_resolve_path(&filename, runtime_context) else {
        return false;
    };
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() == 0)
        .unwrap_or(false)
}

pub(super) fn zip_reset_object(object: &ObjectRef) {
    object.set_property("__zip_path", Value::Null);
    object.set_property("__zip_write_mode", Value::Bool(false));
    object.set_property("__zip_write_entries", Value::Null);
    object.set_property("__zip_comment", Value::string(Vec::new()));
    object.set_property("__zip_archive_flags", Value::Int(0));
    object.set_property("filename", Value::string(Vec::new()));
    object.set_property("numFiles", Value::Int(0));
    object.set_property("status", Value::Int(0));
    object.set_property("statusSys", Value::Int(0));
    object.set_property("lastId", Value::Int(-1));
    object.set_property("comment", Value::string(Vec::new()));
}

fn zip_comment_too_long_value_error(method: &str, argument: &str) -> String {
    format!(
        "E_PHP_VM_SPL_VALUE_ERROR: {method}(): Argument {argument} must be less than 65535 bytes"
    )
}

pub(super) fn zip_object_write_mode(object: &ObjectRef) -> bool {
    matches!(
        object.get_property("__zip_write_mode"),
        Some(Value::Bool(true))
    )
}

pub(super) fn zip_archive_flags(object: &ObjectRef) -> i64 {
    match object.get_property("__zip_archive_flags") {
        Some(Value::Int(flags)) => flags,
        _ => 0,
    }
}

pub(super) fn zip_archive_flag_enabled(object: &ObjectRef, flag: i64) -> bool {
    zip_archive_flags(object) & flag != 0
}

pub(super) fn zip_set_archive_flag(object: &ObjectRef, flag: i64, enabled: bool) {
    let mut flags = zip_archive_flags(object);
    if enabled {
        flags |= flag;
    } else {
        flags &= !flag;
    }
    object.set_property("__zip_archive_flags", Value::Int(flags));
}

pub(super) fn zip_object_readonly(object: &ObjectRef) -> bool {
    zip_archive_flag_enabled(object, ZIP_AFL_RDONLY)
}

pub(super) fn zip_readonly_mutation_value(object: &ObjectRef) -> Option<Value> {
    if zip_object_readonly(object) {
        object.set_property("status", Value::Int(ZIP_ER_RDONLY));
        Some(Value::Bool(false))
    } else {
        None
    }
}

pub(super) fn zip_object_path(object: &ObjectRef) -> Option<PathBuf> {
    match object.get_property("__zip_path") {
        Some(Value::String(path)) if !path.as_bytes().is_empty() => {
            Some(PathBuf::from(path.to_string_lossy()))
        }
        _ => None,
    }
}

pub(super) fn zip_resolve_path(
    path: &str,
    runtime_context: &RuntimeContext,
) -> Result<PathBuf, String> {
    let raw = Path::new(path);
    let resolved = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        runtime_context.cwd.join(raw)
    };
    if !runtime_context.filesystem.allows_path(&resolved) {
        return Err(format!(
            "E_PHP_VM_ZIP_PATH_DENIED: ZipArchive path {} is outside allowed filesystem roots",
            resolved.display()
        ));
    }
    Ok(resolved)
}

pub(super) fn zip_entry_count_for_object(object: &ObjectRef) -> Result<usize, String> {
    if zip_object_write_mode(object) {
        return Ok(zip_pending_entries(object).iter().len());
    }
    let Some(path) = zip_object_path(object) else {
        return Ok(0);
    };
    zip_entry_count(&path)
}

pub(super) fn zip_entry_count(path: &Path) -> Result<usize, String> {
    let file = fs::File::open(path).map_err(|error| {
        format!(
            "E_PHP_VM_ZIP_OPEN: failed to open zip archive `{}`: {error}",
            path.display()
        )
    })?;
    let archive = zip::ZipArchive::new(file).map_err(|error| {
        format!(
            "E_PHP_VM_ZIP_READ: failed to read zip archive `{}`: {error}",
            path.display()
        )
    })?;
    Ok(archive.len())
}

pub(super) fn zip_optional_length(value: Option<&Value>) -> Result<Option<usize>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if matches!(value, Value::Null) {
        return Ok(None);
    }
    let length = to_int(value)?;
    if length <= 0 {
        return Ok(None);
    }
    Ok(Some(length as usize))
}

pub(super) fn zip_read_name(
    path: &Path,
    name: &str,
    max_len: Option<usize>,
) -> Result<Option<Vec<u8>>, String> {
    let mut archive = zip_open_archive(path)?;
    for index in 0..archive.len() {
        let mut file = zip_file_by_index(&mut archive, index, path)?;
        if file.name() == name {
            return zip_read_file(&mut file, max_len).map(Some);
        }
    }
    Ok(None)
}

pub(super) fn zip_read_index(
    path: &Path,
    index: i64,
    max_len: Option<usize>,
) -> Result<Option<Vec<u8>>, String> {
    if index < 0 {
        return Ok(None);
    }
    let mut archive = zip_open_archive(path)?;
    let index = index as usize;
    if index >= archive.len() {
        return Ok(None);
    }
    let mut file = zip_file_by_index(&mut archive, index, path)?;
    zip_read_file(&mut file, max_len).map(Some)
}

pub(super) fn zip_locate_name(
    path: &Path,
    name: &str,
    flags: i64,
) -> Result<Option<usize>, String> {
    let mut archive = zip_open_archive(path)?;
    for index in 0..archive.len() {
        let file = zip_file_by_index(&mut archive, index, path)?;
        if zip_entry_name_matches(file.name(), name, flags) {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

pub(super) fn zip_entry_name_matches(entry_name: &str, requested: &str, flags: i64) -> bool {
    let candidate = if flags & ZIP_FL_NODIR != 0 {
        entry_name.rsplit('/').next().unwrap_or(entry_name)
    } else {
        entry_name
    };
    if flags & ZIP_FL_NOCASE != 0 {
        candidate.eq_ignore_ascii_case(requested)
    } else {
        candidate == requested
    }
}

pub(super) fn zip_stat_name(path: &Path, name: &str) -> Result<Option<PhpArray>, String> {
    let mut archive = zip_open_archive(path)?;
    for index in 0..archive.len() {
        let file = zip_file_by_index(&mut archive, index, path)?;
        if file.name() == name {
            return Ok(Some(zip_file_stat(index, &file)));
        }
    }
    Ok(None)
}

pub(super) fn zip_stat_index(path: &Path, index: i64) -> Result<Option<PhpArray>, String> {
    if index < 0 {
        return Ok(None);
    }
    let mut archive = zip_open_archive(path)?;
    let index = index as usize;
    if index >= archive.len() {
        return Ok(None);
    }
    let file = zip_file_by_index(&mut archive, index, path)?;
    Ok(Some(zip_file_stat(index, &file)))
}

pub(super) fn zip_name_index(path: &Path, index: i64) -> Result<Option<String>, String> {
    if index < 0 {
        return Ok(None);
    }
    let mut archive = zip_open_archive(path)?;
    let index = index as usize;
    if index >= archive.len() {
        return Ok(None);
    }
    let file = zip_file_by_index(&mut archive, index, path)?;
    Ok(Some(file.name().to_owned()))
}

pub(super) fn zip_open_archive(path: &Path) -> Result<zip::ZipArchive<fs::File>, String> {
    let file = fs::File::open(path).map_err(|error| {
        format!(
            "E_PHP_VM_ZIP_OPEN: failed to open zip archive `{}`: {error}",
            path.display()
        )
    })?;
    zip::ZipArchive::new(file).map_err(|error| {
        format!(
            "E_PHP_VM_ZIP_READ: failed to read zip archive `{}`: {error}",
            path.display()
        )
    })
}

pub(super) fn zip_file_by_index<'a>(
    archive: &'a mut zip::ZipArchive<fs::File>,
    index: usize,
    path: &Path,
) -> Result<zip::read::ZipFile<'a>, String> {
    archive.by_index(index).map_err(|error| {
        format!(
            "E_PHP_VM_ZIP_ENTRY_READ: failed to read zip entry {index} from `{}`: {error}",
            path.display()
        )
    })
}

pub(super) fn zip_read_file(
    file: &mut zip::read::ZipFile<'_>,
    max_len: Option<usize>,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("E_PHP_VM_ZIP_ENTRY_READ: failed to read zip entry: {error}"))?;
    if let Some(max_len) = max_len {
        bytes.truncate(max_len);
    }
    Ok(bytes)
}

pub(super) fn zip_normalize_entry_name(name: &str, directory: bool) -> Result<String, String> {
    let mut name = name.replace('\\', "/");
    while let Some(stripped) = name.strip_prefix('/') {
        name = stripped.to_owned();
    }
    if name.is_empty() || name.split('/').any(|part| part == "..") {
        return Err(format!(
            "E_PHP_VM_ZIP_ENTRY_PATH: zip entry name `{name}` is not safe"
        ));
    }
    if directory && !name.ends_with('/') {
        name.push('/');
    }
    Ok(name)
}

pub(super) fn zip_load_write_state(path: &Path) -> Result<(PhpArray, Vec<u8>), String> {
    let mut archive = zip_open_archive(path)?;
    let comment = archive.comment().to_vec();
    let mut entries = PhpArray::new();
    for index in 0..archive.len() {
        let mut file = zip_file_by_index(&mut archive, index, path)?;
        let name = file.name().to_owned();
        let kind = if name.ends_with('/') { "dir" } else { "file" };
        let size = file.size() as i64;
        let compressed_size = file.compressed_size() as i64;
        let crc = file.crc32() as i64;
        let comment = file.comment().as_bytes().to_vec();
        let contents = zip_read_file(&mut file, None)?;
        let key = ArrayKey::String(PhpString::from_bytes(name.as_bytes().to_vec()));
        entries.insert(
            key,
            Value::Array(zip_write_entry(
                kind,
                name,
                contents,
                size,
                compressed_size,
                crc,
                comment,
            )),
        );
    }
    Ok((entries, comment))
}

pub(super) fn zip_write_entry(
    kind: &str,
    name: String,
    contents: Vec<u8>,
    size: i64,
    compressed_size: i64,
    crc: i64,
    comment: Vec<u8>,
) -> PhpArray {
    let mut entry = PhpArray::new();
    zip_array_insert(&mut entry, "kind", Value::string(kind));
    zip_array_insert(&mut entry, "name", Value::string(name.into_bytes()));
    zip_array_insert(&mut entry, "contents", Value::string(contents));
    zip_array_insert(&mut entry, "size", Value::Int(size));
    zip_array_insert(&mut entry, "comp_size", Value::Int(compressed_size));
    zip_array_insert(&mut entry, "crc", Value::Int(crc));
    zip_array_insert(&mut entry, "comment", Value::string(comment));
    entry
}

pub(super) fn zip_pending_entries(object: &ObjectRef) -> PhpArray {
    match object.get_property("__zip_write_entries") {
        Some(Value::Array(entries)) => entries,
        _ => PhpArray::new(),
    }
}

pub(super) fn zip_pending_entry_by_index(
    object: &ObjectRef,
    index: i64,
) -> Result<Option<(usize, PhpArray)>, String> {
    if index < 0 {
        return Ok(None);
    }
    let index = index as usize;
    let entries = zip_pending_entries(object);
    for (position, (_, value)) in entries.iter().enumerate() {
        if position != index {
            continue;
        }
        return match value {
            Value::Array(entry) => Ok(Some((position, entry.clone()))),
            _ => Err(format!(
                "E_PHP_VM_ZIP_WRITE_STATE: zip pending entry {position} must be array, {} found",
                value_type_name(value)
            )),
        };
    }
    Ok(None)
}

pub(super) fn zip_pending_entry_by_name(
    object: &ObjectRef,
    name: &str,
) -> Result<Option<(usize, PhpArray)>, String> {
    let entries = zip_pending_entries(object);
    for (position, (_, value)) in entries.iter().enumerate() {
        let Value::Array(entry) = value else {
            return Err(format!(
                "E_PHP_VM_ZIP_WRITE_STATE: zip pending entry {position} must be array, {} found",
                value_type_name(value)
            ));
        };
        if zip_entry_string(entry, "name")? == name {
            return Ok(Some((position, entry.clone())));
        }
    }
    Ok(None)
}

pub(super) fn zip_pending_name_index(
    object: &ObjectRef,
    index: i64,
) -> Result<Option<String>, String> {
    match zip_pending_entry_by_index(object, index)? {
        Some((_, entry)) => zip_entry_string(&entry, "name").map(Some),
        None => Ok(None),
    }
}

pub(super) fn zip_pending_locate_name(
    object: &ObjectRef,
    name: &str,
    flags: i64,
) -> Result<Option<usize>, String> {
    let entries = zip_pending_entries(object);
    for (position, (_, value)) in entries.iter().enumerate() {
        let Value::Array(entry) = value else {
            return Err(format!(
                "E_PHP_VM_ZIP_WRITE_STATE: zip pending entry {position} must be array, {} found",
                value_type_name(value)
            ));
        };
        if zip_entry_name_matches(&zip_entry_string(entry, "name")?, name, flags) {
            return Ok(Some(position));
        }
    }
    Ok(None)
}

pub(super) fn zip_pending_read_name(
    object: &ObjectRef,
    name: &str,
    max_len: Option<usize>,
) -> Result<Option<Vec<u8>>, String> {
    match zip_pending_entry_by_name(object, name)? {
        Some((_, entry)) => Ok(Some(zip_truncate_bytes(
            zip_entry_bytes(&entry, "contents")?,
            max_len,
        ))),
        None => Ok(None),
    }
}

pub(super) fn zip_pending_read_index(
    object: &ObjectRef,
    index: i64,
    max_len: Option<usize>,
) -> Result<Option<Vec<u8>>, String> {
    match zip_pending_entry_by_index(object, index)? {
        Some((_, entry)) => Ok(Some(zip_truncate_bytes(
            zip_entry_bytes(&entry, "contents")?,
            max_len,
        ))),
        None => Ok(None),
    }
}

pub(super) fn zip_object_comment_value(object: &ObjectRef) -> Value {
    match object.get_property("__zip_comment") {
        Some(Value::String(comment)) => Value::String(comment),
        _ => Value::string(Vec::new()),
    }
}

pub(super) fn zip_pending_comment_index(
    object: &ObjectRef,
    index: i64,
) -> Result<Option<Vec<u8>>, String> {
    match zip_pending_entry_by_index(object, index)? {
        Some((_, entry)) => zip_entry_bytes(&entry, "comment").map(Some),
        None => Ok(None),
    }
}

pub(super) fn zip_pending_comment_name(
    object: &ObjectRef,
    name: &str,
) -> Result<Option<Vec<u8>>, String> {
    match zip_pending_entry_by_name(object, name)? {
        Some((_, entry)) => zip_entry_bytes(&entry, "comment").map(Some),
        None => Ok(None),
    }
}

pub(super) fn zip_set_pending_comment_index(
    object: &ObjectRef,
    index: i64,
    comment: Vec<u8>,
) -> Result<bool, String> {
    if index < 0 {
        return Ok(false);
    }
    let index = index as usize;
    let mut entries = zip_pending_entry_pairs(object)?;
    if index >= entries.len() {
        return Ok(false);
    }
    zip_array_insert(&mut entries[index].1, "comment", Value::string(comment));
    zip_store_pending_entry_pairs(object, entries, index as i64);
    Ok(true)
}

pub(super) fn zip_set_pending_comment_name(
    object: &ObjectRef,
    name: &str,
    comment: Vec<u8>,
) -> Result<bool, String> {
    let mut entries = zip_pending_entry_pairs(object)?;
    let Some(index) = entries.iter().position(|(_, entry)| {
        zip_entry_string(entry, "name").is_ok_and(|entry_name| entry_name == name)
    }) else {
        return Ok(false);
    };
    zip_array_insert(&mut entries[index].1, "comment", Value::string(comment));
    zip_store_pending_entry_pairs(object, entries, index as i64);
    Ok(true)
}

pub(super) fn zip_truncate_bytes(mut bytes: Vec<u8>, max_len: Option<usize>) -> Vec<u8> {
    if let Some(max_len) = max_len {
        bytes.truncate(max_len);
    }
    bytes
}

pub(super) fn zip_pending_stat_index(
    object: &ObjectRef,
    index: i64,
) -> Result<Option<PhpArray>, String> {
    match zip_pending_entry_by_index(object, index)? {
        Some((position, entry)) => zip_pending_entry_stat(position, &entry).map(Some),
        None => Ok(None),
    }
}

pub(super) fn zip_pending_stat_name(
    object: &ObjectRef,
    name: &str,
) -> Result<Option<PhpArray>, String> {
    match zip_pending_entry_by_name(object, name)? {
        Some((position, entry)) => zip_pending_entry_stat(position, &entry).map(Some),
        None => Ok(None),
    }
}

pub(super) fn zip_pending_entry_stat(index: usize, entry: &PhpArray) -> Result<PhpArray, String> {
    let name = zip_entry_string(entry, "name")?;
    let contents = zip_entry_bytes(entry, "contents")?;
    let size = zip_entry_int(entry, "size").unwrap_or(contents.len() as i64);
    let compressed_size = zip_entry_int(entry, "comp_size").unwrap_or(size);
    let crc = zip_entry_int(entry, "crc").unwrap_or(0);
    let mut array = PhpArray::new();
    zip_array_insert(&mut array, "name", Value::string(name.into_bytes()));
    zip_array_insert(&mut array, "index", Value::Int(index as i64));
    zip_array_insert(&mut array, "size", Value::Int(size));
    zip_array_insert(&mut array, "comp_size", Value::Int(compressed_size));
    zip_array_insert(&mut array, "crc", Value::Int(crc));
    Ok(array)
}

pub(super) fn zip_append_write_entry(
    object: &ObjectRef,
    kind: &str,
    name: String,
    contents: Vec<u8>,
    flags: i64,
) -> Result<bool, String> {
    let mut entries = zip_pending_entries(object);
    let key = ArrayKey::String(PhpString::from_bytes(name.as_bytes().to_vec()));
    let existing_index = entries
        .iter()
        .enumerate()
        .find_map(|(index, (entry_key, _))| (entry_key == key.clone()).then_some(index));
    if existing_index.is_some() && flags & ZIP_FL_OVERWRITE == 0 {
        object.set_property("status", Value::Int(ZIP_ER_EXISTS));
        object.set_property("lastId", Value::Int(-1));
        return Ok(false);
    }
    let size = contents.len() as i64;
    let entry = zip_write_entry(kind, name, contents, size, size, 0, Vec::new());
    entries.insert(key, Value::Array(entry));
    let last_id = existing_index.unwrap_or_else(|| entries.iter().len().saturating_sub(1));
    object.set_property("numFiles", Value::Int(entries.iter().len() as i64));
    object.set_property("__zip_write_entries", Value::Array(entries));
    object.set_property("status", Value::Int(0));
    object.set_property("lastId", Value::Int(last_id as i64));
    Ok(true)
}

pub(super) fn zip_delete_pending_index(object: &ObjectRef, index: i64) -> Result<bool, String> {
    if index < 0 {
        return Ok(false);
    }
    let index = index as usize;
    let mut entries = zip_pending_entry_pairs(object)?;
    if index >= entries.len() {
        return Ok(false);
    }
    entries.remove(index);
    zip_store_pending_entry_pairs(object, entries, -1);
    Ok(true)
}

pub(super) fn zip_delete_pending_name(object: &ObjectRef, name: &str) -> Result<bool, String> {
    let mut entries = zip_pending_entry_pairs(object)?;
    let Some(index) = entries.iter().position(|(_, entry)| {
        zip_entry_string(entry, "name").is_ok_and(|entry_name| entry_name == name)
    }) else {
        return Ok(false);
    };
    entries.remove(index);
    zip_store_pending_entry_pairs(object, entries, -1);
    Ok(true)
}

pub(super) fn zip_rename_pending_index(
    object: &ObjectRef,
    index: i64,
    new_name: String,
) -> Result<bool, String> {
    if index < 0 {
        return Ok(false);
    }
    let index = index as usize;
    zip_rename_pending_at(object, index, new_name)
}

pub(super) fn zip_rename_pending_name(
    object: &ObjectRef,
    old_name: &str,
    new_name: String,
) -> Result<bool, String> {
    let entries = zip_pending_entry_pairs(object)?;
    let Some(index) = entries.iter().position(|(_, entry)| {
        zip_entry_string(entry, "name").is_ok_and(|entry_name| entry_name == old_name)
    }) else {
        return Ok(false);
    };
    zip_rename_pending_at(object, index, new_name)
}

pub(super) fn zip_rename_pending_at(
    object: &ObjectRef,
    index: usize,
    new_name: String,
) -> Result<bool, String> {
    let mut entries = zip_pending_entry_pairs(object)?;
    if index >= entries.len() {
        return Ok(false);
    }
    if entries
        .iter()
        .enumerate()
        .any(|(position, (name, _))| position != index && name == &new_name)
    {
        object.set_property("status", Value::Int(ZIP_ER_EXISTS));
        object.set_property("lastId", Value::Int(-1));
        return Ok(false);
    }
    let entry = &mut entries[index].1;
    zip_array_insert(entry, "name", Value::string(new_name.as_bytes().to_vec()));
    entries[index].0 = new_name;
    zip_store_pending_entry_pairs(object, entries, index as i64);
    Ok(true)
}

pub(super) fn zip_pending_entry_pairs(
    object: &ObjectRef,
) -> Result<Vec<(String, PhpArray)>, String> {
    zip_pending_entries(object)
        .iter()
        .enumerate()
        .map(|(position, (_, value))| {
            let Value::Array(entry) = value else {
                return Err(format!(
                    "E_PHP_VM_ZIP_WRITE_STATE: zip pending entry {position} must be array, {} found",
                    value_type_name(value)
                ));
            };
            let name = zip_entry_string(entry, "name")?;
            Ok((name, entry.clone()))
        })
        .collect()
}

pub(super) fn zip_store_pending_entry_pairs(
    object: &ObjectRef,
    entries: Vec<(String, PhpArray)>,
    last_id: i64,
) {
    let mut array = PhpArray::new();
    for (name, entry) in entries {
        array.insert(zip_entry_key(&name), Value::Array(entry));
    }
    object.set_property("numFiles", Value::Int(array.iter().len() as i64));
    object.set_property("__zip_write_entries", Value::Array(array));
    object.set_property("status", Value::Int(0));
    object.set_property("lastId", Value::Int(last_id));
}

pub(super) fn zip_entry_key(name: &str) -> ArrayKey {
    ArrayKey::String(PhpString::from_bytes(name.as_bytes().to_vec()))
}

pub(super) fn zip_write_archive(object: &ObjectRef, path: &Path) -> Result<(), String> {
    let entries = match object.get_property("__zip_write_entries") {
        Some(Value::Array(entries)) => entries,
        _ => PhpArray::new(),
    };
    let mut file = fs::File::create(path).map_err(|error| {
        format!(
            "E_PHP_VM_ZIP_WRITE: failed to create zip archive `{}`: {error}",
            path.display()
        )
    })?;
    let archive_comment = match object.get_property("__zip_comment") {
        Some(Value::String(comment)) => comment.as_bytes().to_vec(),
        _ => Vec::new(),
    };
    if archive_comment.len() > u16::MAX as usize {
        return Err(format!(
            "E_PHP_VM_ZIP_WRITE: archive comment for `{}` is too long",
            path.display()
        ));
    }

    let mut central_directory = Vec::new();
    let mut entry_count = 0usize;
    for (_, value) in entries.iter() {
        let Value::Array(entry) = value else {
            continue;
        };
        let kind = zip_entry_string(entry, "kind")?;
        let name = zip_entry_string(entry, "name")?;
        let contents = zip_entry_bytes(entry, "contents")?;
        let comment = zip_entry_bytes(entry, "comment").unwrap_or_default();
        zip_write_stored_entry(
            &mut file,
            &mut central_directory,
            &kind,
            &name,
            &contents,
            &comment,
        )
        .map_err(|error| {
            format!("E_PHP_VM_ZIP_WRITE: failed to write zip entry `{name}`: {error}")
        })?;
        entry_count += 1;
    }
    zip_write_central_directory(&mut file, &central_directory, entry_count, &archive_comment)
        .map_err(|error| {
            format!(
                "E_PHP_VM_ZIP_WRITE: failed to finish zip archive `{}`: {error}",
                path.display()
            )
        })?;
    Ok(())
}

pub(super) fn zip_write_stored_entry(
    writer: &mut fs::File,
    central_directory: &mut Vec<u8>,
    kind: &str,
    name: &str,
    contents: &[u8],
    comment: &[u8],
) -> Result<(), String> {
    let local_offset = writer
        .stream_position()
        .map_err(|error| format!("failed to read local header offset: {error}"))?;
    let name_bytes = name.as_bytes();
    zip_validate_u16_len(name_bytes.len(), "entry name")?;
    zip_validate_u16_len(comment.len(), "entry comment")?;
    zip_validate_u32_len(contents.len(), "entry contents")?;
    zip_validate_u32_len(local_offset as usize, "local header offset")?;
    let crc = zip_crc32(contents);
    let size = contents.len() as u32;
    let flags = if name.is_ascii() && comment.is_ascii() {
        0u16
    } else {
        0x0800u16
    };
    let external_attrs = if kind == "dir" { 0x10u32 } else { 0u32 };

    zip_write_u32(writer, 0x0403_4b50)?;
    zip_write_u16(writer, 20)?;
    zip_write_u16(writer, flags)?;
    zip_write_u16(writer, 0)?;
    zip_write_u16(writer, 0)?;
    zip_write_u16(writer, 33)?;
    zip_write_u32(writer, crc)?;
    zip_write_u32(writer, size)?;
    zip_write_u32(writer, size)?;
    zip_write_u16(writer, name_bytes.len() as u16)?;
    zip_write_u16(writer, 0)?;
    writer
        .write_all(name_bytes)
        .map_err(|error| format!("failed to write local entry name: {error}"))?;
    writer
        .write_all(contents)
        .map_err(|error| format!("failed to write local entry contents: {error}"))?;

    zip_write_u32(central_directory, 0x0201_4b50)?;
    zip_write_u16(central_directory, 20)?;
    zip_write_u16(central_directory, 20)?;
    zip_write_u16(central_directory, flags)?;
    zip_write_u16(central_directory, 0)?;
    zip_write_u16(central_directory, 0)?;
    zip_write_u16(central_directory, 33)?;
    zip_write_u32(central_directory, crc)?;
    zip_write_u32(central_directory, size)?;
    zip_write_u32(central_directory, size)?;
    zip_write_u16(central_directory, name_bytes.len() as u16)?;
    zip_write_u16(central_directory, 0)?;
    zip_write_u16(central_directory, comment.len() as u16)?;
    zip_write_u16(central_directory, 0)?;
    zip_write_u16(central_directory, 0)?;
    zip_write_u32(central_directory, external_attrs)?;
    zip_write_u32(central_directory, local_offset as u32)?;
    central_directory
        .write_all(name_bytes)
        .map_err(|error| format!("failed to write central entry name: {error}"))?;
    central_directory
        .write_all(comment)
        .map_err(|error| format!("failed to write central entry comment: {error}"))?;
    Ok(())
}

pub(super) fn zip_write_central_directory(
    writer: &mut fs::File,
    central_directory: &[u8],
    entry_count: usize,
    archive_comment: &[u8],
) -> Result<(), String> {
    zip_validate_u16_len(entry_count, "entry count")?;
    zip_validate_u32_len(central_directory.len(), "central directory")?;
    let central_offset = writer
        .stream_position()
        .map_err(|error| format!("failed to read central directory offset: {error}"))?;
    zip_validate_u32_len(central_offset as usize, "central directory offset")?;
    writer
        .write_all(central_directory)
        .map_err(|error| format!("failed to write central directory: {error}"))?;
    zip_write_u32(writer, 0x0605_4b50)?;
    zip_write_u16(writer, 0)?;
    zip_write_u16(writer, 0)?;
    zip_write_u16(writer, entry_count as u16)?;
    zip_write_u16(writer, entry_count as u16)?;
    zip_write_u32(writer, central_directory.len() as u32)?;
    zip_write_u32(writer, central_offset as u32)?;
    zip_write_u16(writer, archive_comment.len() as u16)?;
    writer
        .write_all(archive_comment)
        .map_err(|error| format!("failed to write archive comment: {error}"))?;
    Ok(())
}

pub(super) fn zip_validate_u16_len(value: usize, field: &str) -> Result<(), String> {
    if value > u16::MAX as usize {
        Err(format!("{field} exceeds ZIP16 limit"))
    } else {
        Ok(())
    }
}

pub(super) fn zip_validate_u32_len(value: usize, field: &str) -> Result<(), String> {
    if value > u32::MAX as usize {
        Err(format!("{field} exceeds ZIP32 limit"))
    } else {
        Ok(())
    }
}

pub(super) fn zip_write_u16<W: Write>(writer: &mut W, value: u16) -> Result<(), String> {
    writer
        .write_all(&value.to_le_bytes())
        .map_err(|error| format!("failed to write u16: {error}"))
}

pub(super) fn zip_write_u32<W: Write>(writer: &mut W, value: u32) -> Result<(), String> {
    writer
        .write_all(&value.to_le_bytes())
        .map_err(|error| format!("failed to write u32: {error}"))
}

pub(super) fn zip_crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

pub(super) fn zip_entry_string(entry: &PhpArray, key: &str) -> Result<String, String> {
    match entry.get(&ArrayKey::String(PhpString::from_bytes(
        key.as_bytes().to_vec(),
    ))) {
        Some(Value::String(value)) => Ok(value.to_string_lossy()),
        Some(value) => Err(format!(
            "E_PHP_VM_ZIP_WRITE_STATE: zip write entry field `{key}` must be string, {} found",
            value_type_name(value)
        )),
        None => Err(format!(
            "E_PHP_VM_ZIP_WRITE_STATE: zip write entry field `{key}` is missing"
        )),
    }
}

pub(super) fn zip_entry_bytes(entry: &PhpArray, key: &str) -> Result<Vec<u8>, String> {
    match entry.get(&ArrayKey::String(PhpString::from_bytes(
        key.as_bytes().to_vec(),
    ))) {
        Some(Value::String(value)) => Ok(value.as_bytes().to_vec()),
        Some(value) => Err(format!(
            "E_PHP_VM_ZIP_WRITE_STATE: zip write entry field `{key}` must be string, {} found",
            value_type_name(value)
        )),
        None => Err(format!(
            "E_PHP_VM_ZIP_WRITE_STATE: zip write entry field `{key}` is missing"
        )),
    }
}

pub(super) fn zip_entry_int(entry: &PhpArray, key: &str) -> Option<i64> {
    match entry.get(&ArrayKey::String(PhpString::from_bytes(
        key.as_bytes().to_vec(),
    ))) {
        Some(Value::Int(value)) => Some(*value),
        _ => None,
    }
}

pub(super) fn zip_file_stat(index: usize, file: &zip::read::ZipFile<'_>) -> PhpArray {
    let mut array = PhpArray::new();
    zip_array_insert(
        &mut array,
        "name",
        Value::string(file.name().as_bytes().to_vec()),
    );
    zip_array_insert(&mut array, "index", Value::Int(index as i64));
    zip_array_insert(&mut array, "size", Value::Int(file.size() as i64));
    zip_array_insert(
        &mut array,
        "comp_size",
        Value::Int(file.compressed_size() as i64),
    );
    zip_array_insert(&mut array, "crc", Value::Int(file.crc32() as i64));
    array
}

pub(super) fn zip_extract_entries(
    value: Option<&Value>,
) -> Result<Option<BTreeSet<String>>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if matches!(value, Value::Null) {
        return Ok(None);
    }
    let mut entries = BTreeSet::new();
    match value {
        Value::String(_) => {
            entries.insert(to_string(value)?.to_string_lossy());
        }
        Value::Array(array) => {
            for (_, entry) in array.iter() {
                entries.insert(to_string(entry)?.to_string_lossy());
            }
        }
        _ => {
            return Err(format!(
                "E_PHP_VM_ZIP_TYPE: ZipArchive::extractTo argument 2 must be string, array, or null, {} given",
                value_type_name(value)
            ));
        }
    }
    Ok(Some(entries))
}

pub(super) fn zip_extract_to(
    archive_path: &Path,
    destination: &str,
    entries: Option<BTreeSet<String>>,
    runtime_context: &RuntimeContext,
) -> Result<bool, String> {
    let destination = zip_resolve_path(destination, runtime_context)?;
    if !runtime_context.filesystem.allows_path(&destination) {
        return Err(format!(
            "E_PHP_VM_ZIP_PATH_DENIED: ZipArchive extraction path {} is outside allowed filesystem roots",
            destination.display()
        ));
    }
    fs::create_dir_all(&destination).map_err(|error| {
        format!(
            "E_PHP_VM_ZIP_EXTRACT: failed to create extraction directory `{}`: {error}",
            destination.display()
        )
    })?;

    let mut archive = zip_open_archive(archive_path)?;
    for index in 0..archive.len() {
        let mut file = zip_file_by_index(&mut archive, index, archive_path)?;
        if let Some(entries) = &entries
            && !entries.contains(file.name())
        {
            continue;
        }
        let Some(enclosed_name) = file.enclosed_name() else {
            return Err(format!(
                "E_PHP_VM_ZIP_EXTRACT_PATH: zip entry `{}` has an unsafe path",
                file.name()
            ));
        };
        let output_path = destination.join(enclosed_name);
        if !output_path.starts_with(&destination)
            || !runtime_context.filesystem.allows_path(&output_path)
        {
            return Err(format!(
                "E_PHP_VM_ZIP_EXTRACT_PATH: zip entry `{}` escapes the extraction directory",
                file.name()
            ));
        }
        if file.name().ends_with('/') {
            fs::create_dir_all(&output_path).map_err(|error| {
                format!(
                    "E_PHP_VM_ZIP_EXTRACT: failed to create directory `{}`: {error}",
                    output_path.display()
                )
            })?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "E_PHP_VM_ZIP_EXTRACT: failed to create directory `{}`: {error}",
                    parent.display()
                )
            })?;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|error| {
            format!(
                "E_PHP_VM_ZIP_EXTRACT: failed to read zip entry `{}`: {error}",
                file.name()
            )
        })?;
        fs::write(&output_path, bytes).map_err(|error| {
            format!(
                "E_PHP_VM_ZIP_EXTRACT: failed to write `{}`: {error}",
                output_path.display()
            )
        })?;
    }
    Ok(true)
}

pub(super) fn zip_array_insert(array: &mut PhpArray, key: &str, value: Value) {
    array.insert(
        ArrayKey::String(PhpString::from_bytes(key.as_bytes().to_vec())),
        value,
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PdoDriver {
    Sqlite,
    Mysql,
    Pgsql,
}

impl PdoDriver {
    const fn name(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Mysql => "mysql",
            Self::Pgsql => "pgsql",
        }
    }
}

pub(super) fn new_pdo_object(
    class_name: &str,
    args: Vec<CallArgument>,
    sqlite: &mut php_runtime::SqliteState,
    mysql: &mut php_runtime::MysqlState,
    postgres: &mut php_runtime::PostgresState,
    runtime_context: &RuntimeContext,
) -> Result<ObjectRef, String> {
    match normalize_class_name(class_name).as_str() {
        "pdo" => {
            let values = call_args_to_positional("PDO::__construct", args)?;
            validate_pdo_arg_count("PDO::__construct", values.len(), 1, 4)?;
            let dsn = to_string(&values[0])?.to_string_lossy();
            let _username = values.get(1).map(to_string).transpose()?;
            let _password = values.get(2).map(to_string).transpose()?;
            if let Some(options) = values.get(3)
                && !matches!(options, Value::Array(_) | Value::Null)
            {
                return Err(format!(
                    "E_PHP_VM_PDO_TYPE: PDO::__construct argument 4 must be array or null, {} given",
                    value_type_name(options)
                ));
            }
            let username = values
                .get(1)
                .map(to_string)
                .transpose()?
                .map(|value| value.to_string_lossy())
                .unwrap_or_default();
            let password = values
                .get(2)
                .map(to_string)
                .transpose()?
                .map(|value| value.to_string_lossy())
                .unwrap_or_default();
            let (driver, id) = pdo_open_connection(
                &dsn,
                &username,
                &password,
                sqlite,
                mysql,
                postgres,
                runtime_context,
            )?;
            Ok(pdo_object(driver, id))
        }
        "pdostatement" => Err(
            "E_PHP_VM_PDO_STATEMENT_CONSTRUCT: PDOStatement objects are created by PDO".to_owned(),
        ),
        "pdorow" => Err("E_PHP_VM_PDO_ROW_CONSTRUCT: PDORow cannot be constructed".to_owned()),
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"
        )),
    }
}

pub(super) fn call_pdo_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    sqlite: &mut php_runtime::SqliteState,
    mysql: &mut php_runtime::MysqlState,
    postgres: &mut php_runtime::PostgresState,
    runtime_context: &RuntimeContext,
) -> Result<Value, String> {
    match normalize_class_name(&object.class_name()).as_str() {
        "pdo" => call_pdo_connection_method(
            object,
            method,
            args,
            sqlite,
            mysql,
            postgres,
            runtime_context,
        ),
        "pdostatement" => call_pdo_statement_method(object, method, args, sqlite, mysql, postgres),
        other => Err(format!(
            "E_PHP_VM_PDO_METHOD_GAP: method {other}::{method} is not implemented in the PDO MVP"
        )),
    }
}

pub(super) fn call_pdo_connection_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    sqlite: &mut php_runtime::SqliteState,
    mysql: &mut php_runtime::MysqlState,
    postgres: &mut php_runtime::PostgresState,
    runtime_context: &RuntimeContext,
) -> Result<Value, String> {
    let values = call_args_to_positional(&format!("PDO::{method}"), args)?;
    match method {
        "__construct" => {
            validate_pdo_arg_count("PDO::__construct", values.len(), 1, 4)?;
            let dsn = to_string(&values[0])?.to_string_lossy();
            let username = values
                .get(1)
                .map(to_string)
                .transpose()?
                .map(|value| value.to_string_lossy())
                .unwrap_or_default();
            let password = values
                .get(2)
                .map(to_string)
                .transpose()?
                .map(|value| value.to_string_lossy())
                .unwrap_or_default();
            if let Some(options) = values.get(3)
                && !matches!(options, Value::Array(_) | Value::Null)
            {
                return Err(format!(
                    "E_PHP_VM_PDO_TYPE: PDO::__construct argument 4 must be array or null, {} given",
                    value_type_name(options)
                ));
            }
            let old_driver = pdo_driver(object);
            let (driver, id) = pdo_open_connection(
                &dsn,
                &username,
                &password,
                sqlite,
                mysql,
                postgres,
                runtime_context,
            )?;
            if let Some(old_id) = pdo_connection_id(object) {
                match old_driver {
                    PdoDriver::Sqlite => {
                        sqlite.close(old_id);
                    }
                    PdoDriver::Mysql => {
                        mysql.close(old_id);
                    }
                    PdoDriver::Pgsql => {
                        postgres.close(old_id);
                    }
                }
            }
            pdo_set_driver(object, driver);
            pdo_set_connection_id(object, id);
            Ok(Value::Null)
        }
        "exec" => {
            validate_pdo_arg_count("PDO::exec", values.len(), 1, 1)?;
            let Some(id) = pdo_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            let sql = to_string(&values[0])?.to_string_lossy();
            match pdo_driver(object) {
                PdoDriver::Sqlite => match sqlite.exec_changes(id, &sql) {
                    Some(changes) => Ok(Value::Int(changes)),
                    None => pdo_sqlite_failure(object, sqlite, Some(id)),
                },
                PdoDriver::Mysql => match mysql.exec_changes(id, &sql) {
                    Ok(changes) => Ok(Value::Int(changes)),
                    Err(_) => pdo_mysql_failure(object, mysql, Some(id), None),
                },
                PdoDriver::Pgsql => match postgres.exec_changes(id, &sql) {
                    Ok(changes) => Ok(Value::Int(changes)),
                    Err(_) => pdo_pgsql_failure(object, postgres, Some(id)),
                },
            }
        }
        "begintransaction" => {
            validate_pdo_arg_count("PDO::beginTransaction", values.len(), 0, 0)?;
            let Some(id) = pdo_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            Ok(Value::Bool(match pdo_driver(object) {
                PdoDriver::Sqlite => sqlite.exec(id, "BEGIN"),
                PdoDriver::Mysql => mysql.exec_changes(id, "START TRANSACTION").is_ok(),
                PdoDriver::Pgsql => postgres.exec_changes(id, "BEGIN").is_ok(),
            }))
        }
        "commit" => {
            validate_pdo_arg_count("PDO::commit", values.len(), 0, 0)?;
            let Some(id) = pdo_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            Ok(Value::Bool(match pdo_driver(object) {
                PdoDriver::Sqlite => sqlite.exec(id, "COMMIT"),
                PdoDriver::Mysql => mysql.exec_changes(id, "COMMIT").is_ok(),
                PdoDriver::Pgsql => postgres.exec_changes(id, "COMMIT").is_ok(),
            }))
        }
        "rollback" => {
            validate_pdo_arg_count("PDO::rollBack", values.len(), 0, 0)?;
            let Some(id) = pdo_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            Ok(Value::Bool(match pdo_driver(object) {
                PdoDriver::Sqlite => sqlite.exec(id, "ROLLBACK"),
                PdoDriver::Mysql => mysql.exec_changes(id, "ROLLBACK").is_ok(),
                PdoDriver::Pgsql => postgres.exec_changes(id, "ROLLBACK").is_ok(),
            }))
        }
        "lastinsertid" => {
            validate_pdo_arg_count("PDO::lastInsertId", values.len(), 0, 1)?;
            let Some(id) = pdo_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            Ok(match pdo_driver(object) {
                PdoDriver::Sqlite => sqlite
                    .last_insert_rowid(id)
                    .map(|id| Value::string(id.to_string().into_bytes()))
                    .unwrap_or(Value::Bool(false)),
                PdoDriver::Mysql => {
                    Value::string(mysql.last_insert_id(id).to_string().into_bytes())
                }
                PdoDriver::Pgsql => {
                    let Some(sequence) = values.first() else {
                        return Ok(Value::Bool(false));
                    };
                    let sequence = to_string(sequence)?.to_string_lossy();
                    let sql = format!("SELECT CURRVAL({})", pdo_pgsql_quote(&sequence));
                    match postgres.query(id, &sql) {
                        Ok(Some(result_id)) => {
                            let value = pdo_pgsql_fetch_column(postgres, result_id, 0);
                            postgres.free_result(result_id);
                            match value {
                                Value::Bool(false) => Value::Bool(false),
                                other => Value::string(
                                    to_string_php(&other)
                                        .map_or_else(
                                            |_| String::new(),
                                            |value| value.to_string_lossy(),
                                        )
                                        .into_bytes(),
                                ),
                            }
                        }
                        _ => Value::Bool(false),
                    }
                }
            })
        }
        "query" => {
            validate_pdo_arg_count("PDO::query", values.len(), 1, 4)?;
            let Some(id) = pdo_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            let sql = to_string(&values[0])?.to_string_lossy();
            match pdo_driver(object) {
                PdoDriver::Sqlite => {
                    let Some(result_id) = sqlite.query(id, &sql) else {
                        return pdo_sqlite_failure(object, sqlite, Some(id));
                    };
                    Ok(Value::Object(pdo_statement_object(
                        PdoDriver::Sqlite,
                        id,
                        &sql,
                        pdo_default_fetch_mode(object),
                        pdo_errmode(object),
                        Some(result_id),
                    )))
                }
                PdoDriver::Mysql => match mysql.query(id, &sql) {
                    Ok(result_id) => {
                        let statement = pdo_statement_object(
                            PdoDriver::Mysql,
                            id,
                            &sql,
                            pdo_default_fetch_mode(object),
                            pdo_errmode(object),
                            result_id,
                        );
                        statement
                            .set_property("__pdo_row_count", Value::Int(mysql.affected_rows(id)));
                        Ok(Value::Object(statement))
                    }
                    Err(_) => pdo_mysql_failure(object, mysql, Some(id), None),
                },
                PdoDriver::Pgsql => match postgres.query(id, &sql) {
                    Ok(result_id) => {
                        let statement = pdo_statement_object(
                            PdoDriver::Pgsql,
                            id,
                            &sql,
                            pdo_default_fetch_mode(object),
                            pdo_errmode(object),
                            result_id,
                        );
                        statement.set_property(
                            "__pdo_row_count",
                            Value::Int(postgres.affected_rows(id)),
                        );
                        Ok(Value::Object(statement))
                    }
                    Err(_) => pdo_pgsql_failure(object, postgres, Some(id)),
                },
            }
        }
        "prepare" => {
            validate_pdo_arg_count("PDO::prepare", values.len(), 1, 2)?;
            let Some(id) = pdo_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            let sql = to_string(&values[0])?.to_string_lossy();
            Ok(Value::Object(pdo_statement_object(
                pdo_driver(object),
                id,
                &sql,
                pdo_default_fetch_mode(object),
                pdo_errmode(object),
                None,
            )))
        }
        "errorcode" => {
            validate_pdo_arg_count("PDO::errorCode", values.len(), 0, 0)?;
            Ok(match pdo_driver(object) {
                PdoDriver::Sqlite => pdo_sqlite_error_code(sqlite, pdo_connection_id(object)),
                PdoDriver::Mysql => pdo_mysql_error_code(mysql, pdo_connection_id(object), None),
                PdoDriver::Pgsql => pdo_pgsql_error_code(postgres, pdo_connection_id(object)),
            })
        }
        "errorinfo" => {
            validate_pdo_arg_count("PDO::errorInfo", values.len(), 0, 0)?;
            Ok(match pdo_driver(object) {
                PdoDriver::Sqlite => pdo_sqlite_error_info(sqlite, pdo_connection_id(object)),
                PdoDriver::Mysql => pdo_mysql_error_info(mysql, pdo_connection_id(object), None),
                PdoDriver::Pgsql => pdo_pgsql_error_info(postgres, pdo_connection_id(object)),
            })
        }
        "getattribute" => {
            validate_pdo_arg_count("PDO::getAttribute", values.len(), 1, 1)?;
            let attribute = to_int(&values[0])?;
            Ok(match attribute {
                3 => pdo_int_property(object, "__pdo_errmode", 0),
                16 => Value::string(pdo_driver(object).name()),
                19 => Value::Int(pdo_default_fetch_mode(object)),
                _ => Value::Null,
            })
        }
        "setattribute" => {
            validate_pdo_arg_count("PDO::setAttribute", values.len(), 2, 2)?;
            let attribute = to_int(&values[0])?;
            let value = to_int(&values[1])?;
            match attribute {
                3 => object.set_property("__pdo_errmode", Value::Int(value)),
                19 => object.set_property("__pdo_default_fetch_mode", Value::Int(value)),
                _ => return Ok(Value::Bool(false)),
            }
            Ok(Value::Bool(true))
        }
        "quote" => {
            validate_pdo_arg_count("PDO::quote", values.len(), 1, 2)?;
            let value = to_string(&values[0])?.to_string_lossy();
            Ok(match pdo_driver(object) {
                PdoDriver::Sqlite => Value::string(format!("'{}'", value.replace('\'', "''"))),
                PdoDriver::Mysql => Value::string(pdo_mysql_quote(&value)),
                PdoDriver::Pgsql => Value::string(pdo_pgsql_quote(&value)),
            })
        }
        other => Err(format!(
            "E_PHP_VM_PDO_METHOD_GAP: method PDO::{other} is not implemented in the PDO MVP"
        )),
    }
}

pub(super) fn call_pdo_statement_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    sqlite: &mut php_runtime::SqliteState,
    mysql: &mut php_runtime::MysqlState,
    postgres: &mut php_runtime::PostgresState,
) -> Result<Value, String> {
    let values = call_args_to_positional(&format!("PDOStatement::{method}"), args)?;
    match method {
        "execute" => {
            validate_pdo_arg_count("PDOStatement::execute", values.len(), 0, 1)?;
            let Some(id) = pdo_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            let query = pdo_query_string(object);
            let (query, params) = pdo_statement_execution_params(object, values.first(), &query)?;
            match pdo_driver(object) {
                PdoDriver::Sqlite => {
                    if let Some(result_id) = pdo_result_id(object) {
                        sqlite.finalize_result(result_id);
                        object.unset_property("__pdo_result");
                    }
                    if pdo_query_returns_rows(&query) {
                        let result_id = if params.is_empty() {
                            sqlite.query(id, &query)
                        } else {
                            sqlite.query_params(id, &query, &params)
                        };
                        let Some(result_id) = result_id else {
                            return pdo_sqlite_failure(object, sqlite, Some(id));
                        };
                        object.set_property("__pdo_result", Value::Int(result_id));
                        object.set_property("__pdo_row_count", Value::Int(0));
                    } else {
                        let changes = if params.is_empty() {
                            sqlite.exec_changes(id, &query)
                        } else {
                            sqlite.exec_changes_params(id, &query, &params)
                        };
                        let Some(changes) = changes else {
                            return pdo_sqlite_failure(object, sqlite, Some(id));
                        };
                        object.set_property("__pdo_row_count", Value::Int(changes));
                    }
                }
                PdoDriver::Mysql => {
                    if let Some(result_id) = pdo_result_id(object) {
                        mysql.free_result(result_id);
                        object.unset_property("__pdo_result");
                    }
                    if let Some(statement_id) = pdo_mysql_statement_id(object) {
                        mysql.stmt_close(statement_id);
                        object.unset_property("__pdo_mysql_statement");
                    }
                    let statement_id = match mysql.prepare_statement(id, &query) {
                        Ok(statement_id) => statement_id,
                        Err(_) => return pdo_mysql_failure(object, mysql, Some(id), None),
                    };
                    object.set_property("__pdo_mysql_statement", Value::Int(statement_id));
                    if mysql.stmt_execute(statement_id, &params).is_err() {
                        return pdo_mysql_failure(object, mysql, Some(id), Some(statement_id));
                    }
                    if let Some(result_id) = mysql.stmt_result(statement_id) {
                        object.set_property("__pdo_result", Value::Int(result_id));
                    }
                    object.set_property(
                        "__pdo_row_count",
                        Value::Int(mysql.stmt_affected_rows(statement_id)),
                    );
                }
                PdoDriver::Pgsql => {
                    if let Some(result_id) = pdo_result_id(object) {
                        postgres.free_result(result_id);
                        object.unset_property("__pdo_result");
                    }
                    let query = if params.is_empty() {
                        query
                    } else {
                        pdo_pgsql_rewrite_positional_query(&query)?
                    };
                    match postgres.execute_prepared(id, &query, &params) {
                        Ok(result_id) => {
                            if let Some(result_id) = result_id {
                                object.set_property("__pdo_result", Value::Int(result_id));
                            }
                            object.set_property(
                                "__pdo_row_count",
                                Value::Int(postgres.affected_rows(id)),
                            );
                        }
                        Err(_) => return pdo_pgsql_failure(object, postgres, Some(id)),
                    }
                }
            }
            Ok(Value::Bool(true))
        }
        "bindvalue" | "bindparam" => {
            validate_pdo_arg_count("PDOStatement::bindValue", values.len(), 2, 3)?;
            let key = pdo_param_key(&values[0])?;
            pdo_set_bound_param(object, key, values[1].clone());
            Ok(Value::Bool(true))
        }
        "fetch" => {
            validate_pdo_arg_count("PDOStatement::fetch", values.len(), 0, 3)?;
            let Some(result_id) = pdo_result_id(object) else {
                return Ok(Value::Bool(false));
            };
            let mode = pdo_fetch_mode(object, values.first())?;
            if mode == 7 {
                return Ok(match pdo_driver(object) {
                    PdoDriver::Sqlite => pdo_fetch_column(sqlite, result_id, 0),
                    PdoDriver::Mysql => pdo_mysql_fetch_column(mysql, result_id, 0),
                    PdoDriver::Pgsql => pdo_pgsql_fetch_column(postgres, result_id, 0),
                });
            }
            Ok(match pdo_driver(object) {
                PdoDriver::Sqlite => pdo_fetch_row(sqlite, result_id, mode),
                PdoDriver::Mysql => pdo_mysql_fetch_row(mysql, result_id, mode),
                PdoDriver::Pgsql => pdo_pgsql_fetch_row(postgres, result_id, mode),
            })
        }
        "fetchall" => {
            validate_pdo_arg_count("PDOStatement::fetchAll", values.len(), 0, usize::MAX)?;
            let Some(result_id) = pdo_result_id(object) else {
                return Ok(Value::Array(PhpArray::new()));
            };
            let mode = pdo_fetch_mode(object, values.first())?;
            if mode == 7 {
                let mut rows = PhpArray::new();
                loop {
                    let value = match pdo_driver(object) {
                        PdoDriver::Sqlite => pdo_fetch_column(sqlite, result_id, 0),
                        PdoDriver::Mysql => pdo_mysql_fetch_column(mysql, result_id, 0),
                        PdoDriver::Pgsql => pdo_pgsql_fetch_column(postgres, result_id, 0),
                    };
                    if matches!(value, Value::Bool(false)) {
                        break;
                    }
                    rows.append(value);
                }
                return Ok(Value::Array(rows));
            }
            if mode == 5 {
                let mut rows = PhpArray::new();
                loop {
                    let row = match pdo_driver(object) {
                        PdoDriver::Sqlite => pdo_fetch_row(sqlite, result_id, mode),
                        PdoDriver::Mysql => pdo_mysql_fetch_row(mysql, result_id, mode),
                        PdoDriver::Pgsql => pdo_pgsql_fetch_row(postgres, result_id, mode),
                    };
                    if matches!(row, Value::Bool(false)) {
                        break;
                    }
                    rows.append(row);
                }
                return Ok(Value::Array(rows));
            }
            match pdo_driver(object) {
                PdoDriver::Sqlite => Ok(sqlite.fetch_all(result_id, pdo_sqlite_fetch_mode(mode))),
                PdoDriver::Mysql => {
                    let mut rows = PhpArray::new();
                    loop {
                        let row = mysql.fetch_array(result_id, pdo_mysql_fetch_mode(mode));
                        if matches!(row, Value::Bool(false)) {
                            break;
                        }
                        rows.append(row);
                    }
                    Ok(Value::Array(rows))
                }
                PdoDriver::Pgsql => {
                    let mut rows = PhpArray::new();
                    loop {
                        let row = postgres.fetch_array(result_id, pdo_pgsql_fetch_mode(mode));
                        if matches!(row, Value::Bool(false)) {
                            break;
                        }
                        rows.append(row);
                    }
                    Ok(Value::Array(rows))
                }
            }
        }
        "fetchcolumn" => {
            validate_pdo_arg_count("PDOStatement::fetchColumn", values.len(), 0, 1)?;
            let Some(result_id) = pdo_result_id(object) else {
                return Ok(Value::Bool(false));
            };
            let column = values.first().map(to_int).transpose()?.unwrap_or(0);
            Ok(match pdo_driver(object) {
                PdoDriver::Sqlite => pdo_fetch_column(sqlite, result_id, column),
                PdoDriver::Mysql => pdo_mysql_fetch_column(mysql, result_id, column),
                PdoDriver::Pgsql => pdo_pgsql_fetch_column(postgres, result_id, column),
            })
        }
        "columncount" => {
            validate_pdo_arg_count("PDOStatement::columnCount", values.len(), 0, 0)?;
            Ok(Value::Int(pdo_result_id(object).map_or(
                0,
                |id| match pdo_driver(object) {
                    PdoDriver::Sqlite => sqlite.num_columns(id),
                    PdoDriver::Mysql => mysql.num_fields(id),
                    PdoDriver::Pgsql => postgres.num_fields(id),
                },
            )))
        }
        "rowcount" => {
            validate_pdo_arg_count("PDOStatement::rowCount", values.len(), 0, 0)?;
            Ok(pdo_int_property(object, "__pdo_row_count", 0))
        }
        "closecursor" => {
            validate_pdo_arg_count("PDOStatement::closeCursor", values.len(), 0, 0)?;
            if let Some(result_id) = pdo_result_id(object) {
                match pdo_driver(object) {
                    PdoDriver::Sqlite => {
                        sqlite.finalize_result(result_id);
                    }
                    PdoDriver::Mysql => {
                        if let Some(statement_id) = pdo_mysql_statement_id(object) {
                            mysql.stmt_free_result(statement_id);
                        } else {
                            mysql.free_result(result_id);
                        }
                    }
                    PdoDriver::Pgsql => {
                        postgres.free_result(result_id);
                    }
                }
                object.unset_property("__pdo_result");
            }
            Ok(Value::Bool(true))
        }
        "errorcode" => {
            validate_pdo_arg_count("PDOStatement::errorCode", values.len(), 0, 0)?;
            Ok(match pdo_driver(object) {
                PdoDriver::Sqlite => pdo_sqlite_error_code(sqlite, pdo_connection_id(object)),
                PdoDriver::Mysql => pdo_mysql_error_code(
                    mysql,
                    pdo_connection_id(object),
                    pdo_mysql_statement_id(object),
                ),
                PdoDriver::Pgsql => pdo_pgsql_error_code(postgres, pdo_connection_id(object)),
            })
        }
        "errorinfo" => {
            validate_pdo_arg_count("PDOStatement::errorInfo", values.len(), 0, 0)?;
            Ok(match pdo_driver(object) {
                PdoDriver::Sqlite => pdo_sqlite_error_info(sqlite, pdo_connection_id(object)),
                PdoDriver::Mysql => pdo_mysql_error_info(
                    mysql,
                    pdo_connection_id(object),
                    pdo_mysql_statement_id(object),
                ),
                PdoDriver::Pgsql => pdo_pgsql_error_info(postgres, pdo_connection_id(object)),
            })
        }
        "setfetchmode" => {
            validate_pdo_arg_count("PDOStatement::setFetchMode", values.len(), 1, usize::MAX)?;
            let mode = to_int(&values[0])?;
            object.set_property("__pdo_default_fetch_mode", Value::Int(mode));
            Ok(Value::Bool(true))
        }
        other => Err(format!(
            "E_PHP_VM_PDO_STATEMENT_METHOD_GAP: method PDOStatement::{other} is not implemented in the PDO MVP"
        )),
    }
}

pub(super) fn pdo_object(driver: PdoDriver, connection_id: i64) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&pdo_runtime_class("PDO"), "PDO");
    pdo_set_driver(&object, driver);
    pdo_set_connection_id(&object, connection_id);
    object.set_property("__pdo_errmode", Value::Int(0));
    object.set_property("__pdo_default_fetch_mode", Value::Int(4));
    object
}

pub(super) fn pdo_statement_object(
    driver: PdoDriver,
    connection_id: i64,
    query: &str,
    default_fetch_mode: i64,
    errmode: i64,
    result_id: Option<i64>,
) -> ObjectRef {
    let object =
        ObjectRef::new_with_display_name(&pdo_runtime_class("PDOStatement"), "PDOStatement");
    pdo_set_driver(&object, driver);
    pdo_set_connection_id(&object, connection_id);
    object.set_property("queryString", Value::string(query));
    object.set_property("__pdo_query", Value::string(query));
    object.set_property("__pdo_default_fetch_mode", Value::Int(default_fetch_mode));
    object.set_property("__pdo_errmode", Value::Int(errmode));
    object.set_property("__pdo_row_count", Value::Int(0));
    if let Some(result_id) = result_id {
        object.set_property("__pdo_result", Value::Int(result_id));
    }
    object
}

pub(super) fn pdo_runtime_class(name: &str) -> RuntimeClassEntry {
    RuntimeClassEntry {
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
        flags: RuntimeClassFlags::default(),
    }
}

pub(super) fn pdo_set_connection_id(object: &ObjectRef, id: i64) {
    object.set_property("__pdo_connection", Value::Int(id));
}

pub(super) fn pdo_set_driver(object: &ObjectRef, driver: PdoDriver) {
    object.set_property("__pdo_driver", Value::string(driver.name()));
}

pub(super) fn pdo_driver(object: &ObjectRef) -> PdoDriver {
    match object.get_property("__pdo_driver") {
        Some(Value::String(value)) if value.to_string_lossy().eq_ignore_ascii_case("mysql") => {
            PdoDriver::Mysql
        }
        Some(Value::String(value)) if value.to_string_lossy().eq_ignore_ascii_case("pgsql") => {
            PdoDriver::Pgsql
        }
        _ => PdoDriver::Sqlite,
    }
}

pub(super) fn pdo_connection_id(object: &ObjectRef) -> Option<i64> {
    match object.get_property("__pdo_connection") {
        Some(Value::Int(id)) => Some(id),
        _ => None,
    }
}

pub(super) fn pdo_result_id(object: &ObjectRef) -> Option<i64> {
    match object.get_property("__pdo_result") {
        Some(Value::Int(id)) => Some(id),
        _ => None,
    }
}

pub(super) fn pdo_mysql_statement_id(object: &ObjectRef) -> Option<i64> {
    match object.get_property("__pdo_mysql_statement") {
        Some(Value::Int(id)) => Some(id),
        _ => None,
    }
}

pub(super) fn pdo_query_string(object: &ObjectRef) -> String {
    match object.get_property("__pdo_query") {
        Some(Value::String(value)) => value.to_string_lossy(),
        _ => String::new(),
    }
}

pub(super) fn pdo_default_fetch_mode(object: &ObjectRef) -> i64 {
    match object.get_property("__pdo_default_fetch_mode") {
        Some(Value::Int(mode)) => mode,
        _ => 4,
    }
}

pub(super) fn pdo_fetch_mode(object: &ObjectRef, value: Option<&Value>) -> Result<i64, String> {
    let mode = value.map(to_int).transpose()?.unwrap_or(0);
    Ok(if mode == 0 {
        pdo_default_fetch_mode(object)
    } else {
        mode
    })
}

pub(super) fn pdo_sqlite_fetch_mode(mode: i64) -> i64 {
    match mode {
        2 => php_runtime::SQLITE3_ASSOC,
        3 => php_runtime::SQLITE3_NUM,
        4 | 0 => php_runtime::SQLITE3_BOTH,
        _ => php_runtime::SQLITE3_BOTH,
    }
}

pub(super) fn pdo_mysql_fetch_mode(mode: i64) -> i64 {
    match mode {
        2 => php_runtime::MYSQLI_ASSOC,
        3 => php_runtime::MYSQLI_NUM,
        4 | 0 => php_runtime::MYSQLI_BOTH,
        _ => php_runtime::MYSQLI_BOTH,
    }
}

pub(super) fn pdo_pgsql_fetch_mode(mode: i64) -> i64 {
    match mode {
        2 => php_runtime::PGSQL_ASSOC,
        3 => php_runtime::PGSQL_NUM,
        4 | 0 => php_runtime::PGSQL_BOTH,
        _ => php_runtime::PGSQL_BOTH,
    }
}

pub(super) fn pdo_fetch_column(
    sqlite: &mut php_runtime::SqliteState,
    result_id: i64,
    column: i64,
) -> Value {
    let row = sqlite.fetch_array(result_id, php_runtime::SQLITE3_NUM);
    let Value::Array(array) = row else {
        return row;
    };
    array
        .get(&ArrayKey::Int(column))
        .cloned()
        .unwrap_or(Value::Bool(false))
}

pub(super) fn pdo_fetch_row(
    sqlite: &mut php_runtime::SqliteState,
    result_id: i64,
    mode: i64,
) -> Value {
    if mode == 5 {
        return pdo_assoc_row_to_object(sqlite.fetch_array(result_id, php_runtime::SQLITE3_ASSOC));
    }
    sqlite.fetch_array(result_id, pdo_sqlite_fetch_mode(mode))
}

pub(super) fn pdo_mysql_fetch_column(
    mysql: &mut php_runtime::MysqlState,
    result_id: i64,
    column: i64,
) -> Value {
    let row = mysql.fetch_array(result_id, php_runtime::MYSQLI_NUM);
    let Value::Array(array) = row else {
        return row;
    };
    array
        .get(&ArrayKey::Int(column))
        .cloned()
        .unwrap_or(Value::Bool(false))
}

pub(super) fn pdo_mysql_fetch_row(
    mysql: &mut php_runtime::MysqlState,
    result_id: i64,
    mode: i64,
) -> Value {
    if mode == 5 {
        return pdo_assoc_row_to_object(mysql.fetch_array(result_id, php_runtime::MYSQLI_ASSOC));
    }
    mysql.fetch_array(result_id, pdo_mysql_fetch_mode(mode))
}

pub(super) fn pdo_pgsql_fetch_column(
    postgres: &mut php_runtime::PostgresState,
    result_id: i64,
    column: i64,
) -> Value {
    let row = postgres.fetch_array(result_id, php_runtime::PGSQL_NUM);
    let Value::Array(array) = row else {
        return row;
    };
    array
        .get(&ArrayKey::Int(column))
        .cloned()
        .unwrap_or(Value::Bool(false))
}

pub(super) fn pdo_pgsql_fetch_row(
    postgres: &mut php_runtime::PostgresState,
    result_id: i64,
    mode: i64,
) -> Value {
    if mode == 5 {
        return pdo_assoc_row_to_object(postgres.fetch_array(result_id, php_runtime::PGSQL_ASSOC));
    }
    postgres.fetch_array(result_id, pdo_pgsql_fetch_mode(mode))
}

pub(super) fn pdo_assoc_row_to_object(row: Value) -> Value {
    let Value::Array(array) = row else {
        return row;
    };
    let object = ObjectRef::new_with_display_name(&std_class_entry(), "stdClass");
    for (key, value) in array.iter() {
        let Some(property) = key.as_string() else {
            continue;
        };
        object.set_property(property.to_string_lossy(), value.clone());
    }
    Value::Object(object)
}

pub(super) fn pdo_param_key(value: &Value) -> Result<ArrayKey, String> {
    match value {
        Value::Int(index) if *index >= 1 => Ok(ArrayKey::Int(*index)),
        Value::String(name) if !name.is_empty() => Ok(ArrayKey::String(name.clone())),
        Value::Reference(cell) => pdo_param_key(&cell.get()),
        _ => Err(format!(
            "E_PHP_VM_PDO_PARAM_KEY: unsupported PDO parameter key {}",
            value_type_name(value)
        )),
    }
}

pub(super) fn pdo_set_bound_param(object: &ObjectRef, key: ArrayKey, value: Value) {
    let mut params = match object.get_property("__pdo_bound_params") {
        Some(Value::Array(params)) => params,
        _ => PhpArray::new(),
    };
    params.insert(key, value);
    object.set_property("__pdo_bound_params", Value::Array(params));
}

pub(super) fn pdo_statement_execution_params(
    object: &ObjectRef,
    execute_arg: Option<&Value>,
    query: &str,
) -> Result<(String, Vec<Value>), String> {
    if let Some(Value::Array(params)) = execute_arg
        && !params.is_empty()
    {
        return pdo_query_and_params_from_execute_array(query, params);
    }
    if let Some(Value::Reference(cell)) = execute_arg {
        return pdo_statement_execution_params(object, Some(&cell.get()), query);
    }
    let Some(Value::Array(params)) = object.get_property("__pdo_bound_params") else {
        return Ok((query.to_owned(), Vec::new()));
    };
    pdo_query_and_params_from_bound_array(query, &params)
}

pub(super) fn pdo_query_and_params_from_execute_array(
    query: &str,
    params: &PhpArray,
) -> Result<(String, Vec<Value>), String> {
    let (positional, named) = pdo_split_execute_params(params);
    if !named.is_empty() {
        return pdo_rewrite_named_query(query, &named);
    }
    Ok((query.to_owned(), positional))
}

pub(super) fn pdo_query_and_params_from_bound_array(
    query: &str,
    params: &PhpArray,
) -> Result<(String, Vec<Value>), String> {
    let (positional, named) = pdo_split_bound_params(params);
    if !named.is_empty() {
        return pdo_rewrite_named_query(query, &named);
    }
    Ok((query.to_owned(), positional))
}

pub(super) fn pdo_split_execute_params(params: &PhpArray) -> (Vec<Value>, HashMap<String, Value>) {
    let mut positional = Vec::new();
    let mut named = HashMap::new();
    for (key, value) in params.iter() {
        match key {
            ArrayKey::Int(_) => positional.push(value.clone()),
            ArrayKey::String(name) => {
                named.insert(pdo_normalize_named_param(&name), value.clone());
            }
        }
    }
    (positional, named)
}

pub(super) fn pdo_split_bound_params(params: &PhpArray) -> (Vec<Value>, HashMap<String, Value>) {
    let mut positional_pairs = Vec::new();
    let mut named = HashMap::new();
    for (key, value) in params.iter() {
        match key {
            ArrayKey::Int(index) if index >= 1 => {
                positional_pairs.push((index, value.clone()));
            }
            ArrayKey::Int(_) => {}
            ArrayKey::String(name) => {
                named.insert(pdo_normalize_named_param(&name), value.clone());
            }
        }
    }
    positional_pairs.sort_by_key(|(index, _)| *index);
    let positional = positional_pairs
        .into_iter()
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    (positional, named)
}

pub(super) fn pdo_normalize_named_param(name: &PhpString) -> String {
    name.to_string_lossy()
        .trim_start_matches(':')
        .to_ascii_lowercase()
}

pub(super) fn pdo_rewrite_named_query(
    query: &str,
    named: &HashMap<String, Value>,
) -> Result<(String, Vec<Value>), String> {
    let mut out = String::with_capacity(query.len());
    let mut params = Vec::new();
    let bytes = query.as_bytes();
    let mut i = 0;
    let mut in_single = false;
    let mut in_double = false;
    while i < bytes.len() {
        if in_single {
            let ch = query[i..].chars().next().expect("valid utf-8");
            out.push(ch);
            i += ch.len_utf8();
            if ch == '\'' {
                if query[i..].starts_with('\'') {
                    out.push('\'');
                    i += 1;
                } else {
                    in_single = false;
                }
            }
            continue;
        }
        if in_double {
            let ch = query[i..].chars().next().expect("valid utf-8");
            out.push(ch);
            i += ch.len_utf8();
            if ch == '"' {
                in_double = false;
            }
            continue;
        }
        match bytes[i] {
            b'\'' => {
                out.push('\'');
                in_single = true;
                i += 1;
            }
            b'"' => {
                out.push('"');
                in_double = true;
                i += 1;
            }
            b':' if pdo_named_placeholder_starts(bytes, i) => {
                let start = i + 1;
                let mut end = start + 1;
                while end < bytes.len() && pdo_is_placeholder_continue(bytes[end]) {
                    end += 1;
                }
                let name = query[start..end].to_ascii_lowercase();
                let Some(value) = named.get(&name) else {
                    return Err(format!(
                        "E_PHP_VM_PDO_PARAM_MISSING: missing bound PDO parameter :{name}"
                    ));
                };
                out.push('?');
                params.push(value.clone());
                i = end;
            }
            _ => {
                let ch = query[i..].chars().next().expect("valid utf-8");
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    Ok((out, params))
}

pub(super) fn pdo_named_placeholder_starts(bytes: &[u8], offset: usize) -> bool {
    bytes[offset] == b':'
        && offset
            .checked_sub(1)
            .is_none_or(|previous| bytes[previous] != b':')
        && bytes
            .get(offset + 1)
            .is_some_and(|byte| pdo_is_placeholder_start(*byte))
}

pub(super) fn pdo_is_placeholder_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

pub(super) fn pdo_is_placeholder_continue(byte: u8) -> bool {
    pdo_is_placeholder_start(byte) || byte.is_ascii_digit()
}

pub(super) fn pdo_int_property(object: &ObjectRef, property: &str, default: i64) -> Value {
    match object.get_property(property) {
        Some(Value::Int(value)) => Value::Int(value),
        _ => Value::Int(default),
    }
}

pub(super) fn pdo_errmode(object: &ObjectRef) -> i64 {
    match object.get_property("__pdo_errmode") {
        Some(Value::Int(value)) => value,
        _ => 0,
    }
}

pub(super) fn pdo_sqlite_failure(
    object: &ObjectRef,
    sqlite: &php_runtime::SqliteState,
    connection_id: Option<i64>,
) -> Result<Value, String> {
    let message = connection_id.map_or_else(
        || "not an open SQLite database".to_owned(),
        |id| sqlite.last_error_msg(id),
    );
    if pdo_errmode(object) == 2 {
        Err(format!("E_PHP_VM_PDO_EXCEPTION: {message}"))
    } else {
        Ok(Value::Bool(false))
    }
}

pub(super) fn pdo_sqlite_error_code(
    sqlite: &php_runtime::SqliteState,
    connection_id: Option<i64>,
) -> Value {
    let code = connection_id.map_or(1, |id| sqlite.last_error_code(id));
    if code == 0 {
        Value::string("00000")
    } else {
        Value::string("HY000")
    }
}

pub(super) fn pdo_sqlite_error_info(
    sqlite: &php_runtime::SqliteState,
    connection_id: Option<i64>,
) -> Value {
    let mut info = PhpArray::new();
    let code = connection_id.map_or(1, |id| sqlite.last_error_code(id));
    if code == 0 {
        info.append(Value::string("00000"));
        info.append(Value::Null);
        info.append(Value::Null);
    } else {
        info.append(Value::string("HY000"));
        info.append(Value::Int(code));
        info.append(Value::string(
            connection_id
                .map_or_else(
                    || "not an open SQLite database".to_owned(),
                    |id| sqlite.last_error_msg(id),
                )
                .into_bytes(),
        ));
    }
    Value::Array(info)
}

pub(super) fn pdo_mysql_failure(
    object: &ObjectRef,
    mysql: &php_runtime::MysqlState,
    connection_id: Option<i64>,
    statement_id: Option<i64>,
) -> Result<Value, String> {
    let message = if let Some(statement_id) = statement_id {
        mysql.stmt_error(statement_id)
    } else if let Some(connection_id) = connection_id {
        mysql.error(connection_id)
    } else {
        mysql.connect_error()
    };
    if pdo_errmode(object) == 2 {
        Err(format!("E_PHP_VM_PDO_EXCEPTION: {message}"))
    } else {
        Ok(Value::Bool(false))
    }
}

pub(super) fn pdo_mysql_error_code(
    mysql: &php_runtime::MysqlState,
    connection_id: Option<i64>,
    statement_id: Option<i64>,
) -> Value {
    let errno = if let Some(statement_id) = statement_id {
        mysql.stmt_errno(statement_id)
    } else if let Some(connection_id) = connection_id {
        mysql.errno(connection_id)
    } else {
        mysql.connect_errno()
    };
    if errno == 0 {
        Value::string("00000")
    } else if let Some(statement_id) = statement_id {
        Value::string(mysql.stmt_sqlstate(statement_id))
    } else {
        Value::string("HY000")
    }
}

pub(super) fn pdo_mysql_error_info(
    mysql: &php_runtime::MysqlState,
    connection_id: Option<i64>,
    statement_id: Option<i64>,
) -> Value {
    let mut info = PhpArray::new();
    let errno = if let Some(statement_id) = statement_id {
        mysql.stmt_errno(statement_id)
    } else if let Some(connection_id) = connection_id {
        mysql.errno(connection_id)
    } else {
        mysql.connect_errno()
    };
    if errno == 0 {
        info.append(Value::string("00000"));
        info.append(Value::Null);
        info.append(Value::Null);
    } else {
        let sqlstate = if let Some(statement_id) = statement_id {
            mysql.stmt_sqlstate(statement_id)
        } else {
            "HY000".to_owned()
        };
        let message = if let Some(statement_id) = statement_id {
            mysql.stmt_error(statement_id)
        } else if let Some(connection_id) = connection_id {
            mysql.error(connection_id)
        } else {
            mysql.connect_error()
        };
        info.append(Value::string(sqlstate));
        info.append(Value::Int(errno));
        info.append(Value::string(message.into_bytes()));
    }
    Value::Array(info)
}

pub(super) fn pdo_pgsql_failure(
    object: &ObjectRef,
    postgres: &php_runtime::PostgresState,
    connection_id: Option<i64>,
) -> Result<Value, String> {
    let message = connection_id.map_or_else(
        || postgres.connect_error(),
        |connection_id| postgres.error(connection_id),
    );
    if pdo_errmode(object) == 2 {
        Err(format!("E_PHP_VM_PDO_EXCEPTION: {message}"))
    } else {
        Ok(Value::Bool(false))
    }
}

pub(super) fn pdo_pgsql_error_code(
    postgres: &php_runtime::PostgresState,
    connection_id: Option<i64>,
) -> Value {
    let sqlstate = connection_id.map_or_else(
        || {
            if postgres.connect_errno() == 0 {
                "00000".to_owned()
            } else {
                "HY000".to_owned()
            }
        },
        |connection_id| postgres.sqlstate(connection_id),
    );
    Value::string(sqlstate)
}

pub(super) fn pdo_pgsql_error_info(
    postgres: &php_runtime::PostgresState,
    connection_id: Option<i64>,
) -> Value {
    let mut info = PhpArray::new();
    let sqlstate = match connection_id {
        Some(connection_id) => postgres.sqlstate(connection_id),
        None if postgres.connect_errno() == 0 => "00000".to_owned(),
        None => "HY000".to_owned(),
    };
    let driver_code = connection_id.map_or_else(
        || postgres.connect_errno(),
        |connection_id| {
            if postgres.sqlstate(connection_id) == "00000" {
                0
            } else {
                1
            }
        },
    );
    if sqlstate == "00000" {
        info.append(Value::string("00000"));
        info.append(Value::Null);
        info.append(Value::Null);
    } else {
        let message = connection_id.map_or_else(
            || postgres.connect_error(),
            |connection_id| postgres.error(connection_id),
        );
        info.append(Value::string(sqlstate));
        info.append(Value::Int(driver_code));
        info.append(Value::string(message.into_bytes()));
    }
    Value::Array(info)
}

pub(super) fn pdo_query_returns_rows(query: &str) -> bool {
    let query = query.trim_start().to_ascii_lowercase();
    ["select", "pragma", "with", "values"]
        .iter()
        .any(|prefix| query.starts_with(prefix))
}

pub(super) fn pdo_open_connection(
    dsn: &str,
    username: &str,
    password: &str,
    sqlite: &mut php_runtime::SqliteState,
    mysql: &mut php_runtime::MysqlState,
    postgres: &mut php_runtime::PostgresState,
    runtime_context: &RuntimeContext,
) -> Result<(PdoDriver, i64), String> {
    if dsn.starts_with("sqlite:") {
        let database = pdo_resolve_sqlite_dsn(dsn, runtime_context)?;
        let id = sqlite
            .open(
                &database,
                php_runtime::SQLITE3_OPEN_READWRITE | php_runtime::SQLITE3_OPEN_CREATE,
            )
            .map_err(|message| {
                format!(
                    "E_PHP_VM_PDO_SQLITE_OPEN: could not open PDO SQLite DSN {dsn:?}: {message}"
                )
            })?;
        return Ok((PdoDriver::Sqlite, id));
    }
    if dsn.starts_with("mysql:") {
        let (options, charset) = pdo_mysql_connect_options_from_dsn(dsn, username, password)?;
        let id = mysql
            .connect(&options)
            .map_err(|error| format!("E_PHP_VM_PDO_MYSQL_OPEN: {error}"))?;
        if let Some(charset) = charset
            && let Err(error) = mysql.set_charset(id, &charset)
        {
            mysql.close(id);
            return Err(format!("E_PHP_VM_PDO_MYSQL_CHARSET: {error}"));
        }
        return Ok((PdoDriver::Mysql, id));
    }
    if dsn.starts_with("pgsql:") {
        let options = pdo_pgsql_connect_options_from_dsn(dsn, username, password)?;
        let id = postgres
            .connect(&options)
            .map_err(|error| format!("E_PHP_VM_PDO_PGSQL_OPEN: {error}"))?;
        return Ok((PdoDriver::Pgsql, id));
    }
    Err(format!(
        "E_PHP_VM_PDO_DSN_GAP: only sqlite:, mysql:, and pgsql: PDO DSNs are implemented, got {dsn:?}"
    ))
}

pub(super) fn pdo_resolve_sqlite_dsn(
    dsn: &str,
    runtime_context: &RuntimeContext,
) -> Result<String, String> {
    let Some(database) = dsn.strip_prefix("sqlite:") else {
        return Err(format!(
            "E_PHP_VM_PDO_DSN_GAP: only sqlite:, mysql:, and pgsql: PDO DSNs are implemented, got {dsn:?}"
        ));
    };
    sqlite_resolve_database_path(database, runtime_context)
}

pub(super) fn pdo_mysql_connect_options_from_dsn(
    dsn: &str,
    username: &str,
    password: &str,
) -> Result<(php_runtime::MysqlConnectOptions, Option<String>), String> {
    let Some(body) = dsn.strip_prefix("mysql:") else {
        return Err(format!(
            "E_PHP_VM_PDO_MYSQL_DSN: invalid MySQL PDO DSN {dsn:?}"
        ));
    };
    let mut host = "localhost".to_owned();
    let mut database = None;
    let mut port = None;
    let mut charset = None;
    for segment in body.split(';').filter(|segment| !segment.is_empty()) {
        let Some((key, value)) = segment.split_once('=') else {
            return Err(format!(
                "E_PHP_VM_PDO_MYSQL_DSN: invalid MySQL PDO DSN option {segment:?}"
            ));
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "host" => host = value.trim().to_owned(),
            "dbname" => database = Some(value.trim().to_owned()),
            "port" => {
                port = Some(value.trim().parse::<u16>().map_err(|_| {
                    format!("E_PHP_VM_PDO_MYSQL_DSN: invalid MySQL port {value:?}")
                })?);
            }
            "charset" => charset = Some(value.trim().to_owned()),
            "unix_socket" => {
                return Err(
                    "E_PHP_VM_PDO_MYSQL_DSN_GAP: unix_socket DSNs are not implemented".to_owned(),
                );
            }
            _ => {}
        }
    }
    let options = php_runtime::MysqlConnectOptions::from_parts(
        &host,
        username,
        password,
        database.as_deref(),
        port,
    )
    .map_err(|error| format!("E_PHP_VM_PDO_MYSQL_DSN: {error}"))?;
    Ok((options, charset))
}

pub(super) fn pdo_pgsql_connect_options_from_dsn(
    dsn: &str,
    username: &str,
    password: &str,
) -> Result<php_runtime::PostgresConnectOptions, String> {
    let Some(body) = dsn.strip_prefix("pgsql:") else {
        return Err(format!(
            "E_PHP_VM_PDO_PGSQL_DSN: invalid PostgreSQL PDO DSN {dsn:?}"
        ));
    };
    let mut options: Vec<(String, String)> = Vec::new();
    for segment in body.split(';').filter(|segment| !segment.is_empty()) {
        let Some((key, value)) = segment.split_once('=') else {
            return Err(format!(
                "E_PHP_VM_PDO_PGSQL_DSN: invalid PostgreSQL PDO DSN option {segment:?}"
            ));
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().to_owned();
        match key.as_str() {
            "host" | "hostaddr" | "dbname" | "user" | "password" | "sslmode"
            | "application_name" => options.push((key, value)),
            "port" => {
                value.parse::<u16>().map_err(|_| {
                    format!("E_PHP_VM_PDO_PGSQL_DSN: invalid PostgreSQL port {value:?}")
                })?;
                options.push((key, value));
            }
            "unix_socket" => {
                return Err(
                    "E_PHP_VM_PDO_PGSQL_DSN_GAP: unix_socket DSNs are not implemented".to_owned(),
                );
            }
            _ => {}
        }
    }
    if !username.is_empty() {
        options.retain(|(key, _)| key != "user");
        options.push(("user".to_owned(), username.to_owned()));
    }
    if !password.is_empty() {
        options.retain(|(key, _)| key != "password");
        options.push(("password".to_owned(), password.to_owned()));
    }
    let dsn = options
        .into_iter()
        .map(|(key, value)| format!("{key}={}", pdo_pgsql_dsn_value(&value)))
        .collect::<Vec<_>>()
        .join(" ");
    php_runtime::PostgresConnectOptions::from_dsn(dsn)
        .map_err(|error| format!("E_PHP_VM_PDO_PGSQL_DSN: {error}"))
}

pub(super) fn pdo_pgsql_dsn_value(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/'))
    {
        return value.to_owned();
    }
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for ch in value.chars() {
        match ch {
            '\'' => quoted.push_str("\\'"),
            '\\' => quoted.push_str("\\\\"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('\'');
    quoted
}

pub(super) fn pdo_mysql_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for ch in value.chars() {
        match ch {
            '\0' => quoted.push_str("\\0"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\\' => quoted.push_str("\\\\"),
            '\'' => quoted.push_str("\\'"),
            '"' => quoted.push_str("\\\""),
            '\x1a' => quoted.push_str("\\Z"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('\'');
    quoted
}

pub(super) fn pdo_pgsql_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for ch in value.chars() {
        match ch {
            '\'' => quoted.push_str("''"),
            '\\' => quoted.push_str("\\\\"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('\'');
    quoted
}

pub(super) fn pdo_pgsql_rewrite_positional_query(query: &str) -> Result<String, String> {
    let mut out = String::with_capacity(query.len());
    let bytes = query.as_bytes();
    let mut i = 0;
    let mut param_index = 1;
    let mut in_single = false;
    let mut in_double = false;
    while i < bytes.len() {
        if in_single {
            let ch = query[i..].chars().next().expect("valid utf-8");
            out.push(ch);
            i += ch.len_utf8();
            if ch == '\'' {
                if query[i..].starts_with('\'') {
                    out.push('\'');
                    i += 1;
                } else {
                    in_single = false;
                }
            }
            continue;
        }
        if in_double {
            let ch = query[i..].chars().next().expect("valid utf-8");
            out.push(ch);
            i += ch.len_utf8();
            if ch == '"' {
                in_double = false;
            }
            continue;
        }
        match bytes[i] {
            b'\'' => {
                out.push('\'');
                in_single = true;
                i += 1;
            }
            b'"' => {
                out.push('"');
                in_double = true;
                i += 1;
            }
            b'?' => {
                out.push('$');
                out.push_str(&param_index.to_string());
                param_index += 1;
                i += 1;
            }
            _ => {
                let ch = query[i..].chars().next().expect("valid utf-8");
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    if param_index == 1 {
        return Err("E_PHP_VM_PDO_PGSQL_PARAM: no positional placeholders found".to_owned());
    }
    Ok(out)
}

pub(super) fn pdo_class_constant_value(class_name: &str, constant: &str) -> Option<Value> {
    if normalize_class_name(class_name) != "pdo" {
        return None;
    }
    let value = match constant.to_ascii_uppercase().as_str() {
        "PARAM_NULL" => Value::Int(0),
        "PARAM_INT" => Value::Int(1),
        "PARAM_STR" => Value::Int(2),
        "PARAM_LOB" => Value::Int(3),
        "PARAM_STMT" => Value::Int(4),
        "PARAM_BOOL" => Value::Int(5),
        "FETCH_DEFAULT" => Value::Int(0),
        "FETCH_LAZY" => Value::Int(1),
        "FETCH_ASSOC" => Value::Int(2),
        "FETCH_NUM" => Value::Int(3),
        "FETCH_BOTH" => Value::Int(4),
        "FETCH_OBJ" => Value::Int(5),
        "FETCH_BOUND" => Value::Int(6),
        "FETCH_COLUMN" => Value::Int(7),
        "ATTR_ERRMODE" => Value::Int(3),
        "ATTR_DRIVER_NAME" => Value::Int(16),
        "ATTR_DEFAULT_FETCH_MODE" => Value::Int(19),
        "ERRMODE_SILENT" => Value::Int(0),
        "ERRMODE_WARNING" => Value::Int(1),
        "ERRMODE_EXCEPTION" => Value::Int(2),
        "ERR_NONE" => Value::string("00000"),
        _ => return None,
    };
    Some(value)
}

pub(super) fn is_xml_runtime_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "domdocument"
            | "domnode"
            | "domelement"
            | "domattr"
            | "domtext"
            | "domcomment"
            | "domcdatasection"
            | "domnodelist"
            | "simplexmlelement"
            | "xmlparser"
            | "xmlreader"
            | "xmlwriter"
    )
}

pub(super) fn new_xml_runtime_object(
    class_name: &str,
    args: Vec<CallArgument>,
) -> Result<ObjectRef, String> {
    let class_name = normalize_class_name(class_name);
    match class_name.as_str() {
        "domdocument" => {
            validate_xml_arg_count("DOMDocument::__construct", &args, 0, 0)?;
            Ok(php_runtime::xml::new_dom_document())
        }
        "domnode" => {
            validate_xml_arg_count("DOMNode::__construct", &args, 0, 0)?;
            Ok(php_runtime::xml::new_dom_node())
        }
        "domtext" => {
            validate_xml_arg_count("DOMText::__construct", &args, 0, 1)?;
            let value = args
                .first()
                .map(|arg| xml_string_arg("DOMText::__construct", arg.value.clone()))
                .transpose()?
                .unwrap_or_default();
            Ok(php_runtime::xml::new_dom_text(&value))
        }
        "domcomment" => {
            validate_xml_arg_count("DOMComment::__construct", &args, 0, 1)?;
            let value = args
                .first()
                .map(|arg| xml_string_arg("DOMComment::__construct", arg.value.clone()))
                .transpose()?
                .unwrap_or_default();
            Ok(php_runtime::xml::new_dom_comment(&value))
        }
        "domcdatasection" => {
            validate_xml_arg_count("DOMCdataSection::__construct", &args, 0, 1)?;
            let value = args
                .first()
                .map(|arg| xml_string_arg("DOMCdataSection::__construct", arg.value.clone()))
                .transpose()?
                .unwrap_or_default();
            Ok(php_runtime::xml::new_dom_cdata_section(&value))
        }
        "domattr" => {
            validate_xml_arg_count("DOMAttr::__construct", &args, 1, 2)?;
            let name = xml_string_arg("DOMAttr::__construct", args[0].value.clone())?;
            let value = args
                .get(1)
                .map(|arg| xml_string_arg("DOMAttr::__construct", arg.value.clone()))
                .transpose()?
                .unwrap_or_default();
            Ok(php_runtime::xml::new_dom_attr(&name, &value))
        }
        "domelement" => {
            validate_xml_arg_count("DOMElement::__construct", &args, 1, 1)?;
            let name = xml_string_arg("DOMElement::__construct", args[0].value.clone())?;
            Ok(php_runtime::xml::new_dom_element(
                &php_runtime::xml::XmlElement {
                    name,
                    attributes: Vec::new(),
                    children: Vec::new(),
                },
            ))
        }
        "domnodelist" => {
            validate_xml_arg_count("DOMNodeList::__construct", &args, 0, 0)?;
            Ok(php_runtime::xml::new_dom_node_list(Vec::new()))
        }
        "simplexmlelement" => {
            validate_xml_arg_count("SimpleXMLElement::__construct", &args, 1, 1)?;
            let xml = xml_string_arg("SimpleXMLElement::__construct", args[0].value.clone())?;
            match php_runtime::xml::simplexml_load_string(&xml)? {
                Value::Object(object) => Ok(object),
                _ => Err(
                    "E_PHP_VM_XML_INTERNAL: SimpleXMLElement constructor did not return object"
                        .to_owned(),
                ),
            }
        }
        "xmlparser" => {
            validate_xml_arg_count("XMLParser::__construct", &args, 0, 0)?;
            Ok(php_runtime::xml::new_xml_parser())
        }
        "xmlreader" => {
            validate_xml_arg_count("XMLReader::__construct", &args, 0, 0)?;
            Ok(php_runtime::xml::new_xml_reader())
        }
        "xmlwriter" => {
            validate_xml_arg_count("XMLWriter::__construct", &args, 0, 0)?;
            Ok(php_runtime::xml::new_xml_writer())
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not an XML runtime class"
        )),
    }
}

pub(super) fn call_xml_runtime_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<Value>,
    runtime_context: &RuntimeContext,
) -> Result<Value, String> {
    let class_name = normalize_class_name(&object.class_name());
    let method = normalize_method_name(method);
    match (class_name.as_str(), method.as_str()) {
        ("domdocument", "loadxml") => {
            validate_xml_value_count("DOMDocument::loadXML", &args, 1, 1)?;
            let xml = xml_string_arg("DOMDocument::loadXML", args[0].clone())?;
            php_runtime::xml::dom_document_load_xml(object, &xml)
        }
        ("domdocument", "savexml") => {
            validate_xml_value_count("DOMDocument::saveXML", &args, 0, 0)?;
            Ok(php_runtime::xml::dom_document_save_xml(object))
        }
        ("domdocument", "createelement") => {
            validate_xml_value_count("DOMDocument::createElement", &args, 1, 2)?;
            let name = xml_string_arg("DOMDocument::createElement", args[0].clone())?;
            let value = args
                .get(1)
                .map(|value| xml_string_arg("DOMDocument::createElement", value.clone()))
                .transpose()?;
            Ok(php_runtime::xml::dom_document_create_element(
                &name,
                value.as_deref(),
            ))
        }
        ("domdocument", "createtextnode") => {
            validate_xml_value_count("DOMDocument::createTextNode", &args, 1, 1)?;
            let value = xml_string_arg("DOMDocument::createTextNode", args[0].clone())?;
            Ok(php_runtime::xml::dom_document_create_text_node(&value))
        }
        ("domdocument", "createcomment") => {
            validate_xml_value_count("DOMDocument::createComment", &args, 1, 1)?;
            let value = xml_string_arg("DOMDocument::createComment", args[0].clone())?;
            Ok(php_runtime::xml::dom_document_create_comment(&value))
        }
        ("domdocument", "createcdatasection") => {
            validate_xml_value_count("DOMDocument::createCDATASection", &args, 1, 1)?;
            let value = xml_string_arg("DOMDocument::createCDATASection", args[0].clone())?;
            Ok(php_runtime::xml::dom_document_create_cdata_section(&value))
        }
        ("domdocument", "createattribute") => {
            validate_xml_value_count("DOMDocument::createAttribute", &args, 1, 1)?;
            let name = xml_string_arg("DOMDocument::createAttribute", args[0].clone())?;
            Ok(php_runtime::xml::dom_document_create_attribute(&name))
        }
        ("domdocument", "appendchild") => {
            validate_xml_value_count("DOMDocument::appendChild", &args, 1, 1)?;
            let Value::Object(child) = args[0].clone() else {
                return Err(
                    "E_PHP_VM_XML_TYPE_ERROR: DOMDocument::appendChild expects DOMNode".to_owned(),
                );
            };
            Ok(php_runtime::xml::dom_document_append_child(object, &child))
        }
        ("domdocument", "documentelement") => {
            validate_xml_value_count("DOMDocument::documentElement", &args, 0, 0)?;
            Ok(php_runtime::xml::dom_document_element(object))
        }
        ("domdocument", "getelementsbytagname") => {
            validate_xml_value_count("DOMDocument::getElementsByTagName", &args, 1, 1)?;
            let name = xml_string_arg("DOMDocument::getElementsByTagName", args[0].clone())?;
            Ok(php_runtime::xml::dom_document_get_elements_by_tag_name(
                object, &name,
            ))
        }
        ("domelement", "getelementsbytagname") => {
            validate_xml_value_count("DOMElement::getElementsByTagName", &args, 1, 1)?;
            let name = xml_string_arg("DOMElement::getElementsByTagName", args[0].clone())?;
            Ok(php_runtime::xml::dom_element_get_elements_by_tag_name(
                object, &name,
            ))
        }
        ("domelement", "getattribute") => {
            validate_xml_value_count("DOMElement::getAttribute", &args, 1, 1)?;
            let name = xml_string_arg("DOMElement::getAttribute", args[0].clone())?;
            Ok(php_runtime::xml::dom_element_get_attribute(object, &name))
        }
        ("domelement", "setattribute") => {
            validate_xml_value_count("DOMElement::setAttribute", &args, 2, 2)?;
            let name = xml_string_arg("DOMElement::setAttribute", args[0].clone())?;
            let value = xml_string_arg("DOMElement::setAttribute", args[1].clone())?;
            Ok(php_runtime::xml::dom_element_set_attribute(
                object, &name, &value,
            ))
        }
        ("domelement", "setattributenode") => {
            validate_xml_value_count("DOMElement::setAttributeNode", &args, 1, 1)?;
            let Value::Object(attribute) = args[0].clone() else {
                return Err(
                    "E_PHP_VM_XML_TYPE_ERROR: DOMElement::setAttributeNode expects DOMAttr"
                        .to_owned(),
                );
            };
            Ok(php_runtime::xml::dom_element_set_attribute_node(
                object, &attribute,
            ))
        }
        ("domelement", "appendchild") => {
            validate_xml_value_count("DOMElement::appendChild", &args, 1, 1)?;
            let Value::Object(child) = args[0].clone() else {
                return Err(
                    "E_PHP_VM_XML_TYPE_ERROR: DOMElement::appendChild expects DOMNode".to_owned(),
                );
            };
            Ok(php_runtime::xml::dom_element_append_child(object, &child))
        }
        ("domnodelist", "item") => {
            validate_xml_value_count("DOMNodeList::item", &args, 1, 1)?;
            let index = to_int(&args[0])?;
            Ok(php_runtime::xml::dom_node_list_item(object, index))
        }
        ("simplexmlelement", "asxml") | ("simplexmlelement", "savexml") => {
            let display_method = if method == "savexml" {
                "SimpleXMLElement::saveXML"
            } else {
                "SimpleXMLElement::asXML"
            };
            validate_xml_value_count(display_method, &args, 0, 1)?;
            let xml = php_runtime::xml::simplexml_as_xml(object);
            if let Some(value) = args.first()
                && !matches!(value, Value::Null)
            {
                let path = xml_string_arg(display_method, value.clone())?;
                let Value::String(bytes) = xml else {
                    return Ok(Value::Bool(false));
                };
                return Ok(simplexml_write_xml_file(
                    runtime_context,
                    &path,
                    bytes.as_bytes(),
                ));
            }
            Ok(xml)
        }
        ("simplexmlelement", "attributes") => {
            validate_xml_value_count("SimpleXMLElement::attributes", &args, 0, 0)?;
            Ok(php_runtime::xml::simplexml_attributes(object))
        }
        ("simplexmlelement", "children") => {
            validate_xml_value_count("SimpleXMLElement::children", &args, 0, 0)?;
            Ok(php_runtime::xml::simplexml_children(object))
        }
        ("simplexmlelement", "getname") => {
            validate_xml_value_count("SimpleXMLElement::getName", &args, 0, 0)?;
            Ok(php_runtime::xml::simplexml_get_name(object))
        }
        ("simplexmlelement", "registerxpathnamespace") => {
            validate_xml_value_count("SimpleXMLElement::registerXPathNamespace", &args, 2, 2)?;
            let prefix =
                xml_string_arg("SimpleXMLElement::registerXPathNamespace", args[0].clone())?;
            let namespace =
                xml_string_arg("SimpleXMLElement::registerXPathNamespace", args[1].clone())?;
            Ok(php_runtime::xml::simplexml_register_xpath_namespace(
                object, &prefix, &namespace,
            ))
        }
        ("simplexmlelement", "xpath") => {
            validate_xml_value_count("SimpleXMLElement::xpath", &args, 1, 1)?;
            let expression = xml_string_arg("SimpleXMLElement::xpath", args[0].clone())?;
            Ok(php_runtime::xml::simplexml_xpath(object, &expression))
        }
        ("simplexmlelement", "__tostring") => {
            validate_xml_value_count("SimpleXMLElement::__toString", &args, 0, 0)?;
            Ok(php_runtime::xml::simplexml_text(object))
        }
        ("xmlreader", "xml") => {
            validate_xml_value_count("XMLReader::XML", &args, 1, 1)?;
            let xml = xml_string_arg("XMLReader::XML", args[0].clone())?;
            php_runtime::xml::xml_reader_xml(object, &xml)
        }
        ("xmlreader", "open") => {
            validate_xml_value_count("XMLReader::open", &args, 1, 3)?;
            let path = xml_string_arg("XMLReader::open", args[0].clone())?;
            let source = xml_reader_open_source(&path, runtime_context)?;
            php_runtime::xml::xml_reader_xml(object, &source)
        }
        ("xmlreader", "read") => {
            validate_xml_value_count("XMLReader::read", &args, 0, 0)?;
            Ok(php_runtime::xml::xml_reader_read(object))
        }
        ("xmlreader", "next") => {
            validate_xml_value_count("XMLReader::next", &args, 0, 1)?;
            let name = args
                .first()
                .cloned()
                .map(|value| xml_string_arg("XMLReader::next", value))
                .transpose()?;
            Ok(php_runtime::xml::xml_reader_next(object, name.as_deref()))
        }
        ("xmlreader", "getattribute") => {
            validate_xml_value_count("XMLReader::getAttribute", &args, 1, 1)?;
            let name = xml_string_arg("XMLReader::getAttribute", args[0].clone())?;
            Ok(php_runtime::xml::xml_reader_get_attribute(object, &name))
        }
        ("xmlreader", "getattributeno") => {
            validate_xml_value_count("XMLReader::getAttributeNo", &args, 1, 1)?;
            let index = to_int(&args[0])?;
            Ok(php_runtime::xml::xml_reader_get_attribute_no(object, index))
        }
        ("xmlreader", "lookupnamespace") => {
            validate_xml_value_count("XMLReader::lookupNamespace", &args, 1, 1)?;
            let prefix = xml_string_arg("XMLReader::lookupNamespace", args[0].clone())?;
            Ok(php_runtime::xml::xml_reader_lookup_namespace(
                object, &prefix,
            ))
        }
        ("xmlreader", "movetoattribute") => {
            validate_xml_value_count("XMLReader::moveToAttribute", &args, 1, 1)?;
            let name = xml_string_arg("XMLReader::moveToAttribute", args[0].clone())?;
            Ok(php_runtime::xml::xml_reader_move_to_attribute(
                object, &name,
            ))
        }
        ("xmlreader", "movetoattributeno") => {
            validate_xml_value_count("XMLReader::moveToAttributeNo", &args, 1, 1)?;
            let index = to_int(&args[0])?;
            Ok(php_runtime::xml::xml_reader_move_to_attribute_no(
                object, index,
            ))
        }
        ("xmlreader", "movetofirstattribute") => {
            validate_xml_value_count("XMLReader::moveToFirstAttribute", &args, 0, 0)?;
            Ok(php_runtime::xml::xml_reader_move_to_first_attribute(object))
        }
        ("xmlreader", "movetonextattribute") => {
            validate_xml_value_count("XMLReader::moveToNextAttribute", &args, 0, 0)?;
            Ok(php_runtime::xml::xml_reader_move_to_next_attribute(object))
        }
        ("xmlreader", "movetoelement") => {
            validate_xml_value_count("XMLReader::moveToElement", &args, 0, 0)?;
            Ok(php_runtime::xml::xml_reader_move_to_element(object))
        }
        ("xmlreader", "readstring") => {
            validate_xml_value_count("XMLReader::readString", &args, 0, 0)?;
            Ok(php_runtime::xml::xml_reader_read_string(object))
        }
        ("xmlreader", "readinnerxml") => {
            validate_xml_value_count("XMLReader::readInnerXml", &args, 0, 0)?;
            Ok(php_runtime::xml::xml_reader_read_inner_xml(object))
        }
        ("xmlreader", "readouterxml") => {
            validate_xml_value_count("XMLReader::readOuterXml", &args, 0, 0)?;
            Ok(php_runtime::xml::xml_reader_read_outer_xml(object))
        }
        ("xmlreader", "expand") => {
            validate_xml_value_count("XMLReader::expand", &args, 0, 0)?;
            Ok(php_runtime::xml::xml_reader_expand(object))
        }
        ("xmlreader", "close") => {
            validate_xml_value_count("XMLReader::close", &args, 0, 0)?;
            Ok(php_runtime::xml::xml_reader_close(object))
        }
        ("xmlwriter", "openmemory") => {
            validate_xml_value_count("XMLWriter::openMemory", &args, 0, 0)?;
            Ok(php_runtime::xml::xml_writer_open_memory(object))
        }
        ("xmlwriter", "startdocument") => {
            validate_xml_value_count("XMLWriter::startDocument", &args, 0, 0)?;
            Ok(php_runtime::xml::xml_writer_start_document(object))
        }
        ("xmlwriter", "startelement") => {
            validate_xml_value_count("XMLWriter::startElement", &args, 1, 1)?;
            let name = xml_string_arg("XMLWriter::startElement", args[0].clone())?;
            Ok(php_runtime::xml::xml_writer_start_element(object, &name))
        }
        ("xmlwriter", "writeattribute") => {
            validate_xml_value_count("XMLWriter::writeAttribute", &args, 2, 2)?;
            let name = xml_string_arg("XMLWriter::writeAttribute", args[0].clone())?;
            let value = xml_string_arg("XMLWriter::writeAttribute", args[1].clone())?;
            Ok(php_runtime::xml::xml_writer_write_attribute(
                object, &name, &value,
            ))
        }
        ("xmlwriter", "text") => {
            validate_xml_value_count("XMLWriter::text", &args, 1, 1)?;
            let value = xml_string_arg("XMLWriter::text", args[0].clone())?;
            Ok(php_runtime::xml::xml_writer_text(object, &value))
        }
        ("xmlwriter", "writecomment") => {
            validate_xml_value_count("XMLWriter::writeComment", &args, 1, 1)?;
            let value = xml_string_arg("XMLWriter::writeComment", args[0].clone())?;
            Ok(php_runtime::xml::xml_writer_write_comment(object, &value))
        }
        ("xmlwriter", "writecdata") => {
            validate_xml_value_count("XMLWriter::writeCdata", &args, 1, 1)?;
            let value = xml_string_arg("XMLWriter::writeCdata", args[0].clone())?;
            Ok(php_runtime::xml::xml_writer_write_cdata(object, &value))
        }
        ("xmlwriter", "writeelement") => {
            validate_xml_value_count("XMLWriter::writeElement", &args, 1, 2)?;
            let name = xml_string_arg("XMLWriter::writeElement", args[0].clone())?;
            let value = args
                .get(1)
                .map(|value| xml_string_arg("XMLWriter::writeElement", value.clone()))
                .transpose()?;
            Ok(php_runtime::xml::xml_writer_write_element(
                object,
                &name,
                value.as_deref(),
            ))
        }
        ("xmlwriter", "endelement") => {
            validate_xml_value_count("XMLWriter::endElement", &args, 0, 0)?;
            Ok(php_runtime::xml::xml_writer_end_element(object))
        }
        ("xmlwriter", "enddocument") => {
            validate_xml_value_count("XMLWriter::endDocument", &args, 0, 0)?;
            Ok(php_runtime::xml::xml_writer_end_document(object))
        }
        ("xmlwriter", "outputmemory") => {
            validate_xml_value_count("XMLWriter::outputMemory", &args, 0, 0)?;
            Ok(php_runtime::xml::xml_writer_output_memory(object))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {}::{method} is not implemented in the XML runtime slice",
            object.display_name()
        )),
    }
}

pub(super) fn call_normalizer_static_method(
    method: &str,
    args: Vec<Value>,
) -> Result<Value, String> {
    let method = normalize_method_name(method);
    match method.as_str() {
        "normalize" => {
            validate_xml_value_count("Normalizer::normalize", &args, 1, 2)?;
            let _ = xml_string_arg("Normalizer::normalize", args[0].clone())?;
            let form = args
                .get(1)
                .map(to_int)
                .transpose()?
                .unwrap_or(NORMALIZER_FORM_C);
            if form != NORMALIZER_FORM_C {
                return Err(
                    "E_PHP_RUNTIME_UNSUPPORTED_INTL: Normalizer::normalize only supports NFC"
                        .to_owned(),
                );
            }
            Ok(args[0].clone())
        }
        "isnormalized" => {
            validate_xml_value_count("Normalizer::isNormalized", &args, 1, 2)?;
            let _ = xml_string_arg("Normalizer::isNormalized", args[0].clone())?;
            let form = args
                .get(1)
                .map(to_int)
                .transpose()?
                .unwrap_or(NORMALIZER_FORM_C);
            if form != NORMALIZER_FORM_C {
                return Err(
                    "E_PHP_RUNTIME_UNSUPPORTED_INTL: Normalizer::isNormalized only supports NFC"
                        .to_owned(),
                );
            }
            Ok(Value::Bool(true))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method Normalizer::{method} is not implemented"
        )),
    }
}

pub(super) fn internal_extension_static_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "normalizer" | "locale" | "xmlwriter" | "pdo" | "phar" | "ffi" | "ziparchive"
    )
}

pub(super) fn call_internal_extension_static_method(
    class_name: &str,
    method: &str,
    args: Vec<Value>,
) -> Result<Value, String> {
    match normalize_class_name(class_name).as_str() {
        "normalizer" => call_normalizer_static_method(method, args),
        "locale" => call_locale_static_method(method, args),
        "xmlwriter" => call_xmlwriter_static_method(method, args),
        "pdo" => call_pdo_static_method(method, args),
        "phar" => call_phar_static_method(method, args),
        "ffi" => call_ffi_static_method(method, args),
        "ziparchive" => call_zip_static_method(method, args),
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
        )),
    }
}

pub(super) fn call_zip_static_method(method: &str, args: Vec<Value>) -> Result<Value, String> {
    let method = normalize_method_name(method);
    match method.as_str() {
        "iscompressionmethodsupported" => {
            validate_zip_arg_count("ZipArchive::isCompressionMethodSupported", args.len(), 1, 2)?;
            let method = to_int(&args[0])?;
            let supported = matches!(method, ZIP_CM_STORE | ZIP_CM_DEFLATE);
            Ok(Value::Bool(supported))
        }
        "isencryptionmethodsupported" => {
            validate_zip_arg_count("ZipArchive::isEncryptionMethodSupported", args.len(), 1, 2)?;
            let method = to_int(&args[0])?;
            let supported = matches!(method, ZIP_EM_NONE | ZIP_EM_TRAD_PKWARE);
            Ok(Value::Bool(supported))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method ZipArchive::{method} is not implemented"
        )),
    }
}

pub(super) fn call_xmlwriter_static_method(
    method: &str,
    args: Vec<Value>,
) -> Result<Value, String> {
    let method = normalize_method_name(method);
    match method.as_str() {
        "tomemory" => {
            validate_xml_value_count("XMLWriter::toMemory", &args, 0, 0)?;
            let object = php_runtime::xml::new_xml_writer();
            let _ = php_runtime::xml::xml_writer_open_memory(&object);
            Ok(Value::Object(object))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method XMLWriter::{method} is not implemented"
        )),
    }
}

pub(super) fn call_ffi_static_method(method: &str, _args: Vec<Value>) -> Result<Value, String> {
    let method = normalize_method_name(method);
    match method.as_str() {
        "addr" | "alignof" | "arraytype" | "cast" | "cdef" | "free" | "isnull" | "load"
        | "memcmp" | "memcpy" | "memset" | "new" | "scope" | "sizeof" | "string" | "type"
        | "typeof" => Err(
            "E_PHP_VM_UNSUPPORTED_FFI: FFI is disabled by default; unsafe FFI requires an explicit capability gate"
                .to_owned(),
        ),
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method FFI::{method} is not implemented"
        )),
    }
}

pub(super) fn call_locale_static_method(method: &str, args: Vec<Value>) -> Result<Value, String> {
    let method = normalize_method_name(method);
    match method.as_str() {
        "getprimarylanguage" => {
            validate_xml_value_count("Locale::getPrimaryLanguage", &args, 1, 1)?;
            let locale = xml_string_arg("Locale::getPrimaryLanguage", args[0].clone())?;
            Ok(Value::string(intl_primary_language(&locale)))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method Locale::{method} is not implemented"
        )),
    }
}

pub(super) fn intl_primary_language(locale: &str) -> String {
    let locale = locale.split('@').next().unwrap_or(locale);
    let subtags = locale.split(['-', '_']).collect::<Vec<_>>();
    if subtags.len() >= 3 && subtags[0].eq_ignore_ascii_case("zh") && subtags[1] == "min" {
        return "zh".to_owned();
    }
    if subtags.len() >= 2
        && matches!(subtags[0].to_ascii_lowercase().as_str(), "i" | "zh" | "sgn")
        && subtags[1].chars().all(|ch| ch.is_ascii_lowercase())
    {
        return format!("{}-{}", subtags[0], subtags[1]).to_ascii_lowercase();
    }
    subtags
        .first()
        .copied()
        .unwrap_or(locale)
        .to_ascii_lowercase()
}

pub(super) fn call_pdo_static_method(method: &str, args: Vec<Value>) -> Result<Value, String> {
    let method = normalize_method_name(method);
    match method.as_str() {
        "getavailabledrivers" => {
            validate_pdo_arg_count("PDO::getAvailableDrivers", args.len(), 0, 0)?;
            Ok(Value::Array(PhpArray::from_packed(vec![
                Value::string("mysql"),
                Value::string("pgsql"),
                Value::string("sqlite"),
            ])))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method PDO::{method} is not implemented"
        )),
    }
}

pub(super) fn call_phar_static_method(method: &str, args: Vec<Value>) -> Result<Value, String> {
    let method = normalize_method_name(method);
    match method.as_str() {
        "getsupportedcompression" => {
            validate_phar_arg_count("Phar::getSupportedCompression", args.len(), 0, 0)?;
            let registry = php_std::ExtensionRegistry::standard_library();
            let mut compression = Vec::new();
            if php_std::introspection::extension_loaded(registry, "zlib") {
                compression.push(Value::string("GZ"));
            }
            if php_std::introspection::extension_loaded(registry, "bz2") {
                compression.push(Value::string("BZIP2"));
            }
            Ok(Value::Array(PhpArray::from_packed(compression)))
        }
        "getsupportedsignatures" => {
            validate_phar_arg_count("Phar::getSupportedSignatures", args.len(), 0, 0)?;
            Ok(Value::Array(PhpArray::from_packed(vec![
                Value::string("MD5"),
                Value::string("SHA-1"),
                Value::string("SHA-256"),
                Value::string("SHA-512"),
            ])))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method Phar::{method} is not implemented"
        )),
    }
}

pub(super) fn xml_class_constant_value(class_name: &str, constant: &str) -> Option<Value> {
    if normalize_class_name(class_name) != "xmlreader" {
        return None;
    }
    let value = match constant.to_ascii_uppercase().as_str() {
        "NONE" => php_runtime::xml::XML_READER_NONE,
        "ELEMENT" => php_runtime::xml::XML_READER_ELEMENT,
        "ATTRIBUTE" => php_runtime::xml::XML_READER_ATTRIBUTE,
        "TEXT" => php_runtime::xml::XML_READER_TEXT,
        "END_ELEMENT" => php_runtime::xml::XML_READER_END_ELEMENT,
        _ => return None,
    };
    Some(Value::Int(value))
}

pub(super) fn simplexml_write_xml_file(
    runtime_context: &RuntimeContext,
    path: &str,
    bytes: &[u8],
) -> Value {
    let resolved = spl_file_resolve_path(path, runtime_context);
    if !runtime_context.filesystem.allows_path(&resolved) {
        return Value::Bool(false);
    }
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&resolved)
        .and_then(|mut file| file.write_all(bytes))
        .map_or(Value::Bool(false), |_| Value::Bool(true))
}

pub(super) fn xml_reader_open_source(
    path: &str,
    runtime_context: &RuntimeContext,
) -> Result<String, String> {
    let path = spl_file_resolve_path(path, runtime_context);
    fs::read_to_string(&path).map_err(|error| {
        format!(
            "E_PHP_VM_XML_READER_OPEN: failed to read `{}`: {error}",
            path.to_string_lossy()
        )
    })
}

pub(super) fn xml_string_arg(name: &str, value: Value) -> Result<String, String> {
    to_string(&value)
        .map(|value| value.to_string_lossy())
        .map_err(|message| format!("E_PHP_VM_XML_TYPE_ERROR: {name} expects string: {message}"))
}

pub(super) fn validate_xml_arg_count(
    name: &str,
    args: &[CallArgument],
    min: usize,
    max: usize,
) -> Result<(), String> {
    for arg in args {
        if let Some(name) = &arg.name {
            return Err(format!(
                "E_PHP_VM_XML_NAMED_ARG: XML runtime methods do not accept named argument ${name}"
            ));
        }
    }
    validate_xml_count(name, args.len(), min, max)
}

pub(super) fn validate_xml_value_count(
    name: &str,
    args: &[Value],
    min: usize,
    max: usize,
) -> Result<(), String> {
    validate_xml_count(name, args.len(), min, max)
}

pub(super) fn validate_xml_count(
    name: &str,
    len: usize,
    min: usize,
    max: usize,
) -> Result<(), String> {
    if len < min || len > max {
        let expected = if min == max {
            min.to_string()
        } else {
            format!("{min} to {max}")
        };
        return Err(format!(
            "E_PHP_VM_XML_ARITY: {name} expects {expected} argument(s), {len} given"
        ));
    }
    Ok(())
}

pub(super) fn validate_pdo_arg_count(
    function: &str,
    actual: usize,
    min: usize,
    max: usize,
) -> Result<(), String> {
    if actual < min || actual > max {
        let expected = if min == max {
            min.to_string()
        } else {
            format!("{min} to {max}")
        };
        return Err(format!(
            "E_PHP_VM_PDO_ARITY: {function} expects {expected} argument(s), {actual} given"
        ));
    }
    Ok(())
}

pub(super) fn new_sqlite_object(
    class_name: &str,
    args: Vec<CallArgument>,
    sqlite: &mut php_runtime::SqliteState,
    runtime_context: &RuntimeContext,
) -> Result<ObjectRef, String> {
    let normalized = normalize_class_name(class_name);
    match normalized.as_str() {
        "sqlite3" => {
            let values = call_args_to_positional("SQLite3::__construct", args)?;
            validate_sqlite_arg_count("SQLite3::__construct", values.len(), 1, 3)?;
            let filename = to_string(&values[0])?.to_string_lossy();
            let flags = values
                .get(1)
                .map(to_int)
                .transpose()?
                .unwrap_or(php_runtime::SQLITE3_OPEN_READWRITE | php_runtime::SQLITE3_OPEN_CREATE);
            let _encryption_key = values.get(2).map(to_string).transpose()?;
            let resolved = sqlite_resolve_database_path(&filename, runtime_context)?;
            let id = sqlite.open(&resolved, flags).map_err(|message| {
                format!("E_PHP_VM_SQLITE_OPEN: could not open SQLite3 database {filename:?}: {message}")
            })?;
            let object = ObjectRef::new_with_display_name(
                &sqlite_class("SQLite3"),
                sqlite_display_name(class_name),
            );
            sqlite_set_connection_id(&object, id);
            Ok(object)
        }
        "sqlite3result" => Err(
            "E_PHP_VM_SQLITE_RESULT_CONSTRUCT: SQLite3Result objects are created by SQLite3::query"
                .to_owned(),
        ),
        "sqlite3stmt" => Err(
            "E_PHP_VM_SQLITE_STMT_GAP: SQLite3Stmt prepared statements are not implemented in the SQLite3 MVP"
                .to_owned(),
        ),
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined"
        )),
    }
}

pub(super) fn call_sqlite_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    sqlite: &mut php_runtime::SqliteState,
    runtime_context: &RuntimeContext,
) -> Result<Value, String> {
    let class_name = object.class_name();
    let method = normalize_method_name(method);
    match normalize_class_name(&class_name).as_str() {
        "sqlite3" => call_sqlite3_method(object, &method, args, sqlite, runtime_context),
        "sqlite3result" => call_sqlite3_result_method(object, &method, args, sqlite),
        "sqlite3stmt" => call_sqlite3_stmt_method(object, &method, args, sqlite),
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
        )),
    }
}

pub(super) fn call_sqlite3_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    sqlite: &mut php_runtime::SqliteState,
    runtime_context: &RuntimeContext,
) -> Result<Value, String> {
    let values = call_args_to_positional(&format!("SQLite3::{method}"), args)?;
    match method {
        "__construct" | "open" => {
            validate_sqlite_arg_count("SQLite3::open", values.len(), 1, 3)?;
            let filename = to_string(&values[0])?.to_string_lossy();
            let flags =
                values.get(1).map(to_int).transpose()?.unwrap_or(
                    php_runtime::SQLITE3_OPEN_READWRITE | php_runtime::SQLITE3_OPEN_CREATE,
                );
            let _encryption_key = values.get(2).map(to_string).transpose()?;
            let resolved = sqlite_resolve_database_path(&filename, runtime_context)?;
            let id = sqlite.open(&resolved, flags).map_err(|message| {
                format!(
                    "E_PHP_VM_SQLITE_OPEN: could not open SQLite3 database {filename:?}: {message}"
                )
            })?;
            if let Some(old_id) = sqlite_connection_id(object) {
                sqlite.close(old_id);
            }
            sqlite_set_connection_id(object, id);
            Ok(Value::Null)
        }
        "exec" => {
            validate_sqlite_arg_count("SQLite3::exec", values.len(), 1, 1)?;
            let Some(id) = sqlite_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            let sql = to_string(&values[0])?.to_string_lossy();
            Ok(Value::Bool(sqlite.exec(id, &sql)))
        }
        "query" => {
            validate_sqlite_arg_count("SQLite3::query", values.len(), 1, 1)?;
            let Some(id) = sqlite_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            let sql = to_string(&values[0])?.to_string_lossy();
            Ok(sqlite
                .query(id, &sql)
                .map(sqlite_result_object)
                .unwrap_or(Value::Bool(false)))
        }
        "prepare" => {
            validate_sqlite_arg_count("SQLite3::prepare", values.len(), 1, 1)?;
            let Some(id) = sqlite_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            let sql = to_string(&values[0])?.to_string_lossy();
            Ok(sqlite_stmt_object(id, &sql))
        }
        "querysingle" => {
            validate_sqlite_arg_count("SQLite3::querySingle", values.len(), 1, 2)?;
            let Some(id) = sqlite_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            let sql = to_string(&values[0])?.to_string_lossy();
            let entire_row = values.get(1).map(to_bool).transpose()?.unwrap_or(false);
            Ok(sqlite.query_single(id, &sql, entire_row))
        }
        "lasterrorcode" | "lastextendederrorcode" => {
            validate_sqlite_arg_count("SQLite3::lastErrorCode", values.len(), 0, 0)?;
            Ok(Value::Int(
                sqlite_connection_id(object).map_or(1, |id| sqlite.last_error_code(id)),
            ))
        }
        "lasterrormsg" => {
            validate_sqlite_arg_count("SQLite3::lastErrorMsg", values.len(), 0, 0)?;
            Ok(Value::string(
                sqlite_connection_id(object)
                    .map_or_else(
                        || "not an open SQLite3 database".to_owned(),
                        |id| sqlite.last_error_msg(id),
                    )
                    .into_bytes(),
            ))
        }
        "lastinsertrowid" => {
            validate_sqlite_arg_count("SQLite3::lastInsertRowID", values.len(), 0, 0)?;
            Ok(Value::Int(
                sqlite_connection_id(object)
                    .and_then(|id| sqlite.last_insert_rowid(id))
                    .unwrap_or(0),
            ))
        }
        "changes" => {
            validate_sqlite_arg_count("SQLite3::changes", values.len(), 0, 0)?;
            Ok(Value::Int(
                sqlite_connection_id(object)
                    .and_then(|id| sqlite.changes(id))
                    .unwrap_or(0),
            ))
        }
        "escapestring" => {
            validate_sqlite_arg_count("SQLite3::escapeString", values.len(), 1, 1)?;
            let value = to_string(&values[0])?.to_string_lossy();
            Ok(Value::string(php_runtime::sqlite::escape_string(&value)))
        }
        "busytimeout" => {
            validate_sqlite_arg_count("SQLite3::busyTimeout", values.len(), 1, 1)?;
            let Some(id) = sqlite_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            let milliseconds = to_int(&values[0])?;
            Ok(Value::Bool(sqlite.busy_timeout(id, milliseconds)))
        }
        "close" => {
            validate_sqlite_arg_count("SQLite3::close", values.len(), 0, 0)?;
            let Some(id) = sqlite_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            object.unset_property("__sqlite3_connection");
            Ok(Value::Bool(sqlite.close(id)))
        }
        other => Err(format!(
            "E_PHP_VM_SQLITE_METHOD_GAP: method SQLite3::{other} is not implemented in the SQLite3 MVP"
        )),
    }
}

pub(super) fn call_sqlite3_result_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    sqlite: &mut php_runtime::SqliteState,
) -> Result<Value, String> {
    let values = call_args_to_positional(&format!("SQLite3Result::{method}"), args)?;
    let Some(id) = sqlite_result_id(object) else {
        return Ok(Value::Bool(false));
    };
    match method {
        "fetcharray" => {
            validate_sqlite_arg_count("SQLite3Result::fetchArray", values.len(), 0, 1)?;
            let mode = values
                .first()
                .map(to_int)
                .transpose()?
                .unwrap_or(php_runtime::SQLITE3_BOTH);
            Ok(sqlite.fetch_array(id, mode))
        }
        "fetchall" => {
            validate_sqlite_arg_count("SQLite3Result::fetchAll", values.len(), 0, 1)?;
            let mode = values
                .first()
                .map(to_int)
                .transpose()?
                .unwrap_or(php_runtime::SQLITE3_BOTH);
            Ok(sqlite.fetch_all(id, mode))
        }
        "finalize" => {
            validate_sqlite_arg_count("SQLite3Result::finalize", values.len(), 0, 0)?;
            object.unset_property("__sqlite3_result");
            Ok(Value::Bool(sqlite.finalize_result(id)))
        }
        "reset" => {
            validate_sqlite_arg_count("SQLite3Result::reset", values.len(), 0, 0)?;
            Ok(Value::Bool(sqlite.reset_result(id)))
        }
        "numcolumns" => {
            validate_sqlite_arg_count("SQLite3Result::numColumns", values.len(), 0, 0)?;
            Ok(Value::Int(sqlite.num_columns(id)))
        }
        other => Err(format!(
            "E_PHP_VM_SQLITE_RESULT_METHOD_GAP: method SQLite3Result::{other} is not implemented in the SQLite3 MVP"
        )),
    }
}

pub(super) fn call_sqlite3_stmt_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    sqlite: &mut php_runtime::SqliteState,
) -> Result<Value, String> {
    let values = call_args_to_positional(&format!("SQLite3Stmt::{method}"), args)?;
    match method {
        "bindvalue" | "bindparam" => {
            validate_sqlite_arg_count("SQLite3Stmt::bindValue", values.len(), 2, 3)?;
            let key = sqlite_param_key(&values[0])?;
            sqlite_set_bound_param(object, key, values[1].clone());
            Ok(Value::Bool(true))
        }
        "execute" => {
            validate_sqlite_arg_count("SQLite3Stmt::execute", values.len(), 0, 0)?;
            let Some(id) = sqlite_stmt_connection_id(object) else {
                return Ok(Value::Bool(false));
            };
            if let Some(result_id) = sqlite_stmt_result_id(object) {
                sqlite.finalize_result(result_id);
                object.unset_property("__sqlite3_stmt_result");
            }
            let query = sqlite_stmt_query(object);
            let (query, params) = sqlite_stmt_execution_params(object, &query)?;
            if pdo_query_returns_rows(&query) {
                let result_id = if params.is_empty() {
                    sqlite.query(id, &query)
                } else {
                    sqlite.query_params(id, &query, &params)
                };
                let Some(result_id) = result_id else {
                    return Ok(Value::Bool(false));
                };
                object.set_property("__sqlite3_stmt_result", Value::Int(result_id));
                Ok(sqlite_result_object(result_id))
            } else {
                let changes = if params.is_empty() {
                    sqlite.exec_changes(id, &query)
                } else {
                    sqlite.exec_changes_params(id, &query, &params)
                };
                Ok(Value::Bool(changes.is_some()))
            }
        }
        "reset" | "clear" => {
            validate_sqlite_arg_count("SQLite3Stmt::reset", values.len(), 0, 0)?;
            if let Some(result_id) = sqlite_stmt_result_id(object) {
                sqlite.finalize_result(result_id);
                object.unset_property("__sqlite3_stmt_result");
            }
            Ok(Value::Bool(true))
        }
        "close" => {
            validate_sqlite_arg_count("SQLite3Stmt::close", values.len(), 0, 0)?;
            if let Some(result_id) = sqlite_stmt_result_id(object) {
                sqlite.finalize_result(result_id);
            }
            object.unset_property("__sqlite3_stmt_connection");
            object.unset_property("__sqlite3_stmt_result");
            Ok(Value::Bool(true))
        }
        other => Err(format!(
            "E_PHP_VM_SQLITE_STMT_METHOD_GAP: method SQLite3Stmt::{other} is not implemented in the SQLite3 MVP"
        )),
    }
}

pub(super) fn sqlite_result_object(result_id: i64) -> Value {
    let object = ObjectRef::new_with_display_name(&sqlite_class("SQLite3Result"), "SQLite3Result");
    object.set_property("__sqlite3_result", Value::Int(result_id));
    Value::Object(object)
}

pub(super) fn sqlite_stmt_object(connection_id: i64, query: &str) -> Value {
    let object = ObjectRef::new_with_display_name(&sqlite_class("SQLite3Stmt"), "SQLite3Stmt");
    object.set_property("__sqlite3_stmt_connection", Value::Int(connection_id));
    object.set_property("__sqlite3_stmt_query", Value::string(query));
    Value::Object(object)
}

pub(super) fn sqlite_class(name: &str) -> RuntimeClassEntry {
    RuntimeClassEntry {
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
        flags: RuntimeClassFlags::default(),
    }
}

pub(super) fn sqlite_display_name(class_name: &str) -> String {
    match normalize_class_name(class_name).as_str() {
        "sqlite3" => "SQLite3".to_owned(),
        "sqlite3result" => "SQLite3Result".to_owned(),
        "sqlite3stmt" => "SQLite3Stmt".to_owned(),
        _ => class_name.to_owned(),
    }
}

pub(super) fn sqlite_set_connection_id(object: &ObjectRef, id: i64) {
    object.set_property("__sqlite3_connection", Value::Int(id));
}

pub(super) fn sqlite_connection_id(object: &ObjectRef) -> Option<i64> {
    match object.get_property("__sqlite3_connection") {
        Some(Value::Int(id)) => Some(id),
        _ => None,
    }
}

pub(super) fn sqlite_result_id(object: &ObjectRef) -> Option<i64> {
    match object.get_property("__sqlite3_result") {
        Some(Value::Int(id)) => Some(id),
        _ => None,
    }
}

pub(super) fn sqlite_stmt_connection_id(object: &ObjectRef) -> Option<i64> {
    match object.get_property("__sqlite3_stmt_connection") {
        Some(Value::Int(id)) => Some(id),
        _ => None,
    }
}

pub(super) fn sqlite_stmt_result_id(object: &ObjectRef) -> Option<i64> {
    match object.get_property("__sqlite3_stmt_result") {
        Some(Value::Int(id)) => Some(id),
        _ => None,
    }
}

pub(super) fn sqlite_stmt_query(object: &ObjectRef) -> String {
    match object.get_property("__sqlite3_stmt_query") {
        Some(Value::String(value)) => value.to_string_lossy(),
        _ => String::new(),
    }
}

pub(super) fn sqlite_param_key(value: &Value) -> Result<ArrayKey, String> {
    match value {
        Value::Int(index) if *index >= 1 => Ok(ArrayKey::Int(*index)),
        Value::String(name) if !name.is_empty() => Ok(ArrayKey::String(name.clone())),
        Value::Reference(cell) => sqlite_param_key(&cell.get()),
        _ => Err(format!(
            "E_PHP_VM_SQLITE_PARAM_KEY: unsupported SQLite3Stmt parameter key {}",
            value_type_name(value)
        )),
    }
}

pub(super) fn sqlite_set_bound_param(object: &ObjectRef, key: ArrayKey, value: Value) {
    let mut params = match object.get_property("__sqlite3_stmt_bound_params") {
        Some(Value::Array(params)) => params,
        _ => PhpArray::new(),
    };
    params.insert(key, value);
    object.set_property("__sqlite3_stmt_bound_params", Value::Array(params));
}

pub(super) fn sqlite_stmt_execution_params(
    object: &ObjectRef,
    query: &str,
) -> Result<(String, Vec<Value>), String> {
    let Some(Value::Array(params)) = object.get_property("__sqlite3_stmt_bound_params") else {
        return Ok((query.to_owned(), Vec::new()));
    };
    let (positional, named) = pdo_split_bound_params(&params);
    if !named.is_empty() {
        return pdo_rewrite_named_query(query, &named);
    }
    Ok((query.to_owned(), positional))
}

pub(super) fn sqlite_resolve_database_path(
    filename: &str,
    runtime_context: &RuntimeContext,
) -> Result<String, String> {
    if filename == ":memory:" {
        return Ok(filename.to_owned());
    }
    let raw = Path::new(filename);
    let path = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        runtime_context.cwd.join(raw)
    };
    if !runtime_context.filesystem.allows_path(&path) {
        return Err(format!(
            "E_PHP_VM_SQLITE_PATH_DENIED: SQLite3 database path {} is outside allowed filesystem roots",
            path.display()
        ));
    }
    Ok(path.to_string_lossy().into_owned())
}

pub(super) fn validate_sqlite_arg_count(
    function: &str,
    actual: usize,
    min: usize,
    max: usize,
) -> Result<(), String> {
    if actual < min || actual > max {
        return Err(format!(
            "E_PHP_VM_SQLITE_ARG_COUNT: {function} expects {min}..{max} argument(s), {actual} given"
        ));
    }
    Ok(())
}

pub(super) fn call_date_time_like_method(
    object: ObjectRef,
    method: &str,
    values: Vec<Value>,
    immutable: bool,
) -> Result<Value, String> {
    let class_name = object.class_name();
    match normalize_method_name(method).as_str() {
        "format" => {
            validate_date_time_arg_count(&format!("{class_name}::format"), values.len(), 1, 1)?;
            let format = to_string(&values[0])?.to_string_lossy();
            let timestamp = php_runtime::datetime::object_timestamp(&object).unwrap_or(0);
            let timezone = php_runtime::datetime::object_timezone(&object)
                .unwrap_or_else(|| php_runtime::datetime::DEFAULT_TIMEZONE.to_owned());
            Ok(Value::string(php_runtime::datetime::format_timestamp(
                timestamp, &timezone, &format,
            )))
        }
        "gettimestamp" => {
            validate_date_time_arg_count(
                &format!("{class_name}::getTimestamp"),
                values.len(),
                0,
                0,
            )?;
            Ok(Value::Int(
                php_runtime::datetime::object_timestamp(&object).unwrap_or(0),
            ))
        }
        "gettimezone" => {
            validate_date_time_arg_count(
                &format!("{class_name}::getTimezone"),
                values.len(),
                0,
                0,
            )?;
            let timezone = php_runtime::datetime::object_timezone(&object)
                .unwrap_or_else(|| php_runtime::datetime::DEFAULT_TIMEZONE.to_owned());
            Ok(php_runtime::datetime::datetimezone_object(&timezone).unwrap_or(Value::Bool(false)))
        }
        "getoffset" => {
            validate_date_time_arg_count(&format!("{class_name}::getOffset"), values.len(), 0, 0)?;
            let timezone = php_runtime::datetime::object_timezone(&object)
                .unwrap_or_else(|| php_runtime::datetime::DEFAULT_TIMEZONE.to_owned());
            Ok(Value::Int(php_runtime::datetime::timezone_offset_seconds(
                &timezone,
            )))
        }
        "settimezone" => {
            validate_date_time_arg_count(
                &format!("{class_name}::setTimezone"),
                values.len(),
                1,
                1,
            )?;
            let timezone = date_time_timezone_name_from_value(&values[0])?;
            php_runtime::datetime::with_timezone(&object, &timezone, immutable).ok_or_else(|| {
                format!("E_PHP_VM_DATETIMEZONE_INVALID: timezone {timezone:?} is unsupported")
            })
        }
        "add" => {
            validate_date_time_arg_count(&format!("{class_name}::add"), values.len(), 1, 1)?;
            let seconds = date_interval_seconds_from_value(&values[0])?;
            Ok(php_runtime::datetime::add_interval(
                &object, seconds, immutable,
            ))
        }
        "sub" => {
            validate_date_time_arg_count(&format!("{class_name}::sub"), values.len(), 1, 1)?;
            let seconds = date_interval_seconds_from_value(&values[0])?;
            Ok(php_runtime::datetime::add_interval(
                &object,
                seconds.saturating_neg(),
                immutable,
            ))
        }
        "modify" => {
            validate_date_time_arg_count(&format!("{class_name}::modify"), values.len(), 1, 1)?;
            let modifier = to_string(&values[0])?.to_string_lossy();
            Ok(
                php_runtime::datetime::modify_object(&object, &modifier, immutable)
                    .unwrap_or(Value::Bool(false)),
            )
        }
        "diff" => {
            validate_date_time_arg_count(&format!("{class_name}::diff"), values.len(), 1, 1)?;
            let Value::Object(right) = effective_value(&values[0]) else {
                return Err(format!(
                    "E_PHP_VM_DATETIME_ARG_TYPE: {class_name}::diff expects DateTimeInterface, {} given",
                    value_type_name(&values[0])
                ));
            };
            if !matches!(
                normalize_class_name(&right.class_name()).as_str(),
                "datetime" | "datetimeimmutable"
            ) {
                return Err(format!(
                    "E_PHP_VM_DATETIME_ARG_TYPE: {class_name}::diff expects DateTimeInterface, {} given",
                    right.class_name()
                ));
            }
            Ok(php_runtime::datetime::diff_objects(&object, &right))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
        )),
    }
}

pub(super) fn call_date_timezone_method(
    object: ObjectRef,
    method: &str,
    values: Vec<Value>,
) -> Result<Value, String> {
    match normalize_method_name(method).as_str() {
        "getname" => {
            validate_date_time_arg_count("DateTimeZone::getName", values.len(), 0, 0)?;
            Ok(php_runtime::datetime::object_timezone(&object)
                .map_or(Value::Bool(false), Value::string))
        }
        "getoffset" => {
            validate_date_time_arg_count("DateTimeZone::getOffset", values.len(), 1, 1)?;
            let Value::Object(datetime) = effective_value(&values[0]) else {
                return Err(format!(
                    "E_PHP_VM_DATETIMEZONE_ARG_TYPE: DateTimeZone::getOffset expects DateTimeInterface, {} given",
                    value_type_name(&values[0])
                ));
            };
            if !matches!(
                normalize_class_name(&datetime.class_name()).as_str(),
                "datetime" | "datetimeimmutable"
            ) {
                return Err(format!(
                    "E_PHP_VM_DATETIMEZONE_ARG_TYPE: DateTimeZone::getOffset expects DateTimeInterface, {} given",
                    datetime.class_name()
                ));
            }
            let timezone = php_runtime::datetime::object_timezone(&object)
                .unwrap_or_else(|| php_runtime::datetime::DEFAULT_TIMEZONE.to_owned());
            Ok(Value::Int(php_runtime::datetime::timezone_offset_seconds(
                &timezone,
            )))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
            object.class_name(),
            method
        )),
    }
}

pub(super) fn call_date_interval_method(
    object: ObjectRef,
    method: &str,
    values: Vec<Value>,
) -> Result<Value, String> {
    match normalize_method_name(method).as_str() {
        "format" => {
            validate_date_time_arg_count("DateInterval::format", values.len(), 1, 1)?;
            let format = to_string(&values[0])?.to_string_lossy();
            let seconds = date_interval_seconds(&object)?;
            Ok(Value::string(php_runtime::datetime::format_interval(
                seconds, &format,
            )))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
            object.class_name(),
            method
        )),
    }
}

pub(super) fn validate_date_time_arg_count(
    name: &str,
    given: usize,
    min: usize,
    max: usize,
) -> Result<(), String> {
    if given < min {
        return Err(format!(
            "E_PHP_VM_TOO_FEW_ARGS: {name} expects at least {min} argument(s), {given} given"
        ));
    }
    if given > max {
        return Err(format!(
            "E_PHP_VM_TOO_MANY_ARGS: {name} expects at most {max} argument(s), {given} given"
        ));
    }
    Ok(())
}

pub(super) fn date_time_timezone_name_from_value(value: &Value) -> Result<String, String> {
    let Value::Object(object) = effective_value(value) else {
        return Err(format!(
            "E_PHP_VM_DATETIMEZONE_ARG_TYPE: expected DateTimeZone, {} given",
            value_type_name(value)
        ));
    };
    if normalize_class_name(&object.class_name()) != "datetimezone" {
        return Err(format!(
            "E_PHP_VM_DATETIMEZONE_ARG_TYPE: expected DateTimeZone, {} given",
            object.class_name()
        ));
    }
    php_runtime::datetime::object_timezone(&object)
        .ok_or_else(|| "E_PHP_VM_DATETIMEZONE_INVALID: DateTimeZone object has no name".to_owned())
}

pub(super) fn date_interval_seconds_from_value(value: &Value) -> Result<i64, String> {
    let Value::Object(object) = effective_value(value) else {
        return Err(format!(
            "E_PHP_VM_DATEINTERVAL_ARG_TYPE: expected DateInterval, {} given",
            value_type_name(value)
        ));
    };
    if normalize_class_name(&object.class_name()) != "dateinterval" {
        return Err(format!(
            "E_PHP_VM_DATEINTERVAL_ARG_TYPE: expected DateInterval, {} given",
            object.class_name()
        ));
    }
    date_interval_seconds(&object)
}

pub(super) fn date_interval_seconds(object: &ObjectRef) -> Result<i64, String> {
    match object.get_property("__seconds") {
        Some(Value::Int(seconds)) => Ok(seconds),
        _ => Err("E_PHP_VM_DATEINTERVAL_INVALID: DateInterval object has no seconds".to_owned()),
    }
}

pub(super) fn date_time_object_from_value(value: Value) -> Result<ObjectRef, String> {
    match value {
        Value::Object(object) => Ok(object),
        _ => Err("E_PHP_VM_DATETIME_OBJECT: helper did not create an object".to_owned()),
    }
}

pub(super) fn date_time_display_name(class_name: &str) -> &'static str {
    match normalize_class_name(class_name).as_str() {
        "datetime" => "DateTime",
        "datetimeimmutable" => "DateTimeImmutable",
        "datetimezone" => "DateTimeZone",
        "dateinterval" => "DateInterval",
        _ => "DateTime",
    }
}

pub(super) fn date_time_class_constant_value(class_name: &str, constant: &str) -> Option<Value> {
    match normalize_class_name(class_name).as_str() {
        "datetimeinterface" | "datetime" | "datetimeimmutable" => {
            date_time_format_class_constant_value(constant)
        }
        "datetimezone" => date_time_zone_class_constant_value(constant),
        _ => None,
    }
}

pub(super) fn date_time_format_class_constant_value(constant: &str) -> Option<Value> {
    Some(Value::string(
        match normalize_class_name(constant).as_str() {
            "atom" => php_std::constants::DATE_ATOM,
            "cookie" => php_std::constants::DATE_COOKIE,
            "iso8601" => php_std::constants::DATE_ISO8601,
            "iso8601_expanded" => php_std::constants::DATE_ISO8601_EXPANDED,
            "rfc822" => php_std::constants::DATE_RFC822,
            "rfc850" => php_std::constants::DATE_RFC850,
            "rfc1036" => php_std::constants::DATE_RFC1036,
            "rfc1123" => php_std::constants::DATE_RFC1123,
            "rfc7231" => php_std::constants::DATE_RFC7231,
            "rfc2822" => php_std::constants::DATE_RFC2822,
            "rfc3339" => php_std::constants::DATE_RFC3339,
            "rfc3339_extended" => php_std::constants::DATE_RFC3339_EXTENDED,
            "rss" => php_std::constants::DATE_RSS,
            "w3c" => php_std::constants::DATE_W3C,
            _ => return None,
        },
    ))
}

pub(super) fn date_time_zone_class_constant_value(constant: &str) -> Option<Value> {
    Some(Value::Int(match normalize_class_name(constant).as_str() {
        "africa" => 1,
        "america" => 2,
        "antarctica" => 4,
        "arctic" => 8,
        "asia" => 16,
        "atlantic" => 32,
        "australia" => 64,
        "europe" => 128,
        "indian" => 256,
        "pacific" => 512,
        "utc" => 1024,
        "all" => 2047,
        "all_with_bc" => 4095,
        "per_country" => 4096,
        _ => return None,
    }))
}
