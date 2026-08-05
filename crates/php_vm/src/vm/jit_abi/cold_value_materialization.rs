//! Cold Rust `Value` materialization for publication and dynamic-unit setup.
//!
//! Generated tiers never import this module. It only translates immutable IR
//! constants/types at cold publication boundaries and formats cold metadata.

use super::*;
use php_runtime::api::PhpString;
use php_runtime::api::Value;

pub(super) fn ir_constant_value(constant: &php_ir::IrConstant) -> Result<Value, String> {
    match constant {
        php_ir::IrConstant::Null => Ok(Value::Null),
        php_ir::IrConstant::Bool(value) => Ok(Value::Bool(*value)),
        php_ir::IrConstant::Int(value) => Ok(Value::Int(*value)),
        php_ir::IrConstant::Float(value) => Ok(Value::float(*value)),
        php_ir::IrConstant::String(value) => Ok(Value::String(PhpString::from_bytes(
            value.as_bytes().to_vec(),
        ))),
        php_ir::IrConstant::StringBytes(value) => {
            Ok(Value::String(PhpString::from_bytes(value.clone())))
        }
        php_ir::IrConstant::Array(entries) => {
            let mut array = php_runtime::api::PhpArray::new();
            for entry in entries {
                let value = ir_constant_value(&entry.value)?;
                if let Some(key) = &entry.key {
                    let key = ir_constant_value(key)?;
                    let key = php_runtime::api::ArrayKey::from_value(&key)
                        .ok_or_else(|| "native constant array key is invalid".to_owned())?;
                    array.insert(key, value);
                } else {
                    array
                        .try_append(value)
                        .map_err(|error| format!("E_PHP_THROW:Error:{error}"))?;
                }
            }
            Ok(Value::Array(array))
        }
        other => Err(format!(
            "native constant {other:?} requires runtime resolution"
        )),
    }
}

pub(super) fn native_runtime_constant_value(
    context: &NativeRequestColdState<'_>,
    constant: &php_ir::IrConstant,
) -> Result<Value, String> {
    fn resolve(
        context: &NativeRequestColdState<'_>,
        constant: &php_ir::IrConstant,
        depth: usize,
    ) -> Result<Value, String> {
        if depth > 32 {
            return Err("native constant resolution exceeded its recursion limit".to_owned());
        }
        match constant {
            php_ir::IrConstant::NamedConstant(name) => context.lookup_constant(name),
            php_ir::IrConstant::ClassConstant {
                class_name,
                display_class_name: _,
                constant_name,
            } => {
                let normalized = normalize_class_name(class_name);
                if let Some(entry) = context
                    .unit
                    .classes
                    .iter()
                    .find(|class| class.name == normalized)
                    .and_then(|class| {
                        class
                            .constants
                            .iter()
                            .find(|entry| entry.name.eq_ignore_ascii_case(constant_name))
                    })
                {
                    if let Some(value) = entry
                        .value
                        .and_then(|id| context.unit.constants.get(id.index()))
                    {
                        return resolve(context, value, depth + 1);
                    }
                    if let Some(reference) = &entry.value_named_constant {
                        for name in &reference.names {
                            if let Ok(value) = context.lookup_constant(name) {
                                return Ok(value);
                            }
                        }
                    }
                }
                if let Some((unit, class)) = native_external_class_handle(context, &normalized)
                    && let Some(entry) = class
                        .constants
                        .iter()
                        .find(|entry| entry.name.eq_ignore_ascii_case(constant_name))
                    && let Some(value) = entry.value.and_then(|id| {
                        context
                            .dynamic_units
                            .get(unit)
                            .and_then(|package| package.compiled.unit().constants.get(id.index()))
                    })
                {
                    return resolve(context, value, depth + 1);
                }
                Err(format!("Undefined constant {class_name}::{constant_name}"))
            }
            php_ir::IrConstant::Array(entries) => {
                let mut array = php_runtime::api::PhpArray::new();
                for entry in entries {
                    let value = resolve(context, &entry.value, depth + 1)?;
                    if let Some(key) = &entry.key {
                        let key = resolve(context, key, depth + 1)?;
                        let key = php_runtime::api::ArrayKey::from_value(&key)
                            .ok_or_else(|| "native constant array key is invalid".to_owned())?;
                        array.insert(key, value);
                    } else {
                        array
                            .try_append(value)
                            .map_err(|error| format!("E_PHP_THROW:Error:{error}"))?;
                    }
                }
                Ok(Value::Array(array))
            }
            value => ir_constant_value(value),
        }
    }
    resolve(context, constant, 0)
}

pub(super) fn native_runtime_type(type_: &php_ir::IrReturnType) -> php_runtime::api::RuntimeType {
    use php_ir::IrReturnType as Ir;
    use php_runtime::api::RuntimeType as Runtime;
    match type_ {
        Ir::Int => Runtime::Int,
        Ir::Float => Runtime::Float,
        Ir::String => Runtime::String,
        Ir::Array => Runtime::Array,
        Ir::Callable => Runtime::Callable,
        Ir::Iterable => Runtime::Iterable,
        Ir::Object => Runtime::Object,
        Ir::Bool => Runtime::Bool,
        Ir::Null => Runtime::Null,
        Ir::Void => Runtime::Void,
        Ir::Mixed => Runtime::Mixed,
        Ir::Never => Runtime::Never,
        Ir::False => Runtime::False,
        Ir::True => Runtime::True,
        Ir::Class { name, display_name } => Runtime::Class {
            name: name.clone(),
            display_name: display_name.clone(),
        },
        Ir::Nullable { inner } => Runtime::Nullable {
            inner: Box::new(native_runtime_type(inner)),
        },
        Ir::Union { members } => Runtime::Union {
            members: members.iter().map(native_runtime_type).collect(),
        },
        Ir::Intersection { members } => Runtime::Intersection {
            members: members.iter().map(native_runtime_type).collect(),
        },
        Ir::Dnf { members } => Runtime::Dnf {
            clauses: members.iter().map(native_runtime_type).collect(),
        },
    }
}

pub(super) fn native_ir_type_name(type_: &php_ir::IrReturnType) -> String {
    use php_ir::IrReturnType as Ir;
    match type_ {
        Ir::Int => "int".to_owned(),
        Ir::Float => "float".to_owned(),
        Ir::String => "string".to_owned(),
        Ir::Array => "array".to_owned(),
        Ir::Callable => "callable".to_owned(),
        Ir::Iterable => "iterable".to_owned(),
        Ir::Object => "object".to_owned(),
        Ir::Bool => "bool".to_owned(),
        Ir::Null => "null".to_owned(),
        Ir::Void => "void".to_owned(),
        Ir::Mixed => "mixed".to_owned(),
        Ir::Never => "never".to_owned(),
        Ir::False => "false".to_owned(),
        Ir::True => "true".to_owned(),
        Ir::Class { display_name, name } => display_name.clone().unwrap_or_else(|| name.clone()),
        Ir::Nullable { inner } => format!("?{}", native_ir_type_name(inner)),
        Ir::Union { members } => {
            let mut names = members.iter().map(native_ir_type_name).collect::<Vec<_>>();
            if let (Some(int), Some(string)) = (
                names.iter().position(|name| name == "int"),
                names.iter().position(|name| name == "string"),
            ) && int < string
            {
                names.swap(int, string);
            }
            names.join("|")
        }
        Ir::Intersection { members } => members
            .iter()
            .map(native_ir_type_name)
            .collect::<Vec<_>>()
            .join("&"),
        Ir::Dnf { members } => members
            .iter()
            .map(native_ir_type_name)
            .collect::<Vec<_>>()
            .join("|"),
    }
}

pub(super) fn dereference_native_assignment_value(mut value: Value) -> Value {
    for _ in 0..16 {
        let Value::Reference(reference) = value else {
            break;
        };
        value = reference.get();
    }
    value
}
