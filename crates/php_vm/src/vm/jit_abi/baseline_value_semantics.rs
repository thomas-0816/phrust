//! Baseline-only Rust `Value` semantic helpers.
//!
//! Native optimizing and exact operations use encoded values, direct
//! dimensions, typed native coercion, and prepared instanceof plans.

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

pub(super) fn native_value_matches_ir_type(value: &Value, type_: &php_ir::IrReturnType) -> bool {
    use php_ir::IrReturnType as Ir;
    let value = match value {
        Value::Reference(reference) => {
            return native_value_matches_ir_type(&reference.get(), type_);
        }
        value => value,
    };
    match type_ {
        Ir::Int => matches!(value, Value::Int(_)),
        Ir::Float => matches!(value, Value::Float(_) | Value::Int(_)),
        Ir::String => matches!(value, Value::String(_)),
        Ir::Array => matches!(value, Value::Array(_)),
        Ir::Callable => matches!(value, Value::Callable(_)),
        Ir::Iterable => matches!(
            value,
            Value::Array(_) | Value::Object(_) | Value::Generator(_)
        ),
        Ir::Object | Ir::Class { .. } => matches!(
            value,
            Value::Object(_) | Value::Callable(_) | Value::Generator(_) | Value::Fiber(_)
        ),
        Ir::Bool => matches!(value, Value::Bool(_)),
        Ir::Null | Ir::Void => matches!(value, Value::Null),
        Ir::Mixed => true,
        Ir::Never => false,
        Ir::False => matches!(value, Value::Bool(false)),
        Ir::True => matches!(value, Value::Bool(true)),
        Ir::Nullable { inner } => {
            matches!(value, Value::Null) || native_value_matches_ir_type(value, inner)
        }
        Ir::Union { members } => members
            .iter()
            .any(|member| native_value_matches_ir_type(value, member)),
        Ir::Intersection { members } => members
            .iter()
            .all(|member| native_value_matches_ir_type(value, member)),
        Ir::Dnf { members } => members
            .iter()
            .any(|member| native_value_matches_ir_type(value, member)),
    }
}

pub(super) fn native_value_matches_ir_type_in_context(
    context: &NativeRequestColdState<'_>,
    value: &Value,
    type_: &php_ir::IrReturnType,
) -> bool {
    use php_ir::IrReturnType as Ir;
    let value = match value {
        Value::Reference(reference) => {
            return native_value_matches_ir_type_in_context(context, &reference.get(), type_);
        }
        value => value,
    };
    match type_ {
        Ir::Class { name, .. } => match value {
            Value::Object(object) => native_class_is_a(context, &object.class_name(), name),
            Value::Callable(_) => name.eq_ignore_ascii_case("Closure"),
            Value::Generator(_) => matches!(
                normalize_class_name(name).as_str(),
                "generator" | "iterator" | "traversable"
            ),
            Value::Fiber(_) => name.eq_ignore_ascii_case("Fiber"),
            _ => false,
        },
        Ir::Iterable => match value {
            Value::Array(_) | Value::Generator(_) => true,
            Value::Object(object) => {
                native_class_is_a(context, &object.class_name(), "traversable")
            }
            _ => false,
        },
        Ir::Nullable { inner } => {
            matches!(value, Value::Null)
                || native_value_matches_ir_type_in_context(context, value, inner)
        }
        Ir::Union { members } | Ir::Dnf { members } => members
            .iter()
            .any(|member| native_value_matches_ir_type_in_context(context, value, member)),
        Ir::Intersection { members } => members
            .iter()
            .all(|member| native_value_matches_ir_type_in_context(context, value, member)),
        _ => native_value_matches_ir_type(value, type_),
    }
}

pub(super) fn native_value_is_callable(
    context: &NativeRequestColdState<'_>,
    value: &Value,
) -> bool {
    match value {
        Value::Reference(reference) => native_value_is_callable(context, &reference.get()),
        Value::Callable(_) => true,
        Value::Object(object) => {
            native_method_in_hierarchy(context, &object.class_name(), "__invoke").is_some()
                || native_external_method(context, &object.class_name(), "__invoke").is_some()
        }
        Value::String(name) => {
            let name = name.to_string_lossy();
            if let Some((class, method)) = name.split_once("::") {
                native_method_in_hierarchy(context, class, method).is_some()
                    || native_external_method(context, class, method).is_some()
            } else {
                context.function_id(&name).is_some()
                    || context.external_function(&name).is_some()
                    || php_extensions::BuiltinRegistry::new().contains(&name.to_ascii_lowercase())
            }
        }
        Value::Array(array) if array.len() == 2 => {
            let target = array.get(&php_runtime::api::ArrayKey::Int(0));
            let method = array.get(&php_runtime::api::ArrayKey::Int(1));
            match (target, method) {
                (Some(Value::Object(object)), Some(Value::String(method))) => {
                    let class = object.class_name();
                    native_method_in_hierarchy(context, &class, &method.to_string_lossy()).is_some()
                        || native_external_method(context, &class, &method.to_string_lossy())
                            .is_some()
                }
                (Some(Value::String(class)), Some(Value::String(method))) => {
                    let class = class.to_string_lossy();
                    native_method_in_hierarchy(context, &class, &method.to_string_lossy()).is_some()
                        || native_external_method(context, &class, &method.to_string_lossy())
                            .is_some()
                }
                _ => false,
            }
        }
        _ => false,
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

impl<'a> NativeRequestColdState<'a> {
    pub(super) fn native_scalar_encoding(&mut self, value: &Value) -> Option<i64> {
        matches!(
            value,
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::Uninitialized
        )
        .then(|| self.encode_baseline_value(value.clone()).ok())
        .flatten()
    }
}

pub(super) fn native_dimension_path_value(
    context: &mut NativeRequestColdState<'_>,
    mut value: Option<Value>,
    arguments: &[i64],
    dimension_count: usize,
    source: &php_ir::Instruction,
    operation: NativeDimensionOperation,
) -> Result<Option<Value>, String> {
    if arguments.len() != dimension_count {
        return Ok(None);
    }
    for encoded in arguments {
        let Some(mut target) = value else {
            return Ok(None);
        };
        while let Value::Reference(reference) = target {
            target = reference.get();
        }
        let mut key = context.decode_baseline_value(*encoded)?;
        while let Value::Reference(reference) = key {
            key = reference.get();
        }
        emit_native_dimension_conversion_diagnostic(
            context,
            &target,
            &key,
            Some(source),
            operation,
        )?;
        let Some(key) = php_runtime::api::ArrayKey::from_value(&key) else {
            return Ok(None);
        };
        value = match target {
            Value::Array(array) => array.get(&key).cloned(),
            Value::Object(object) => native_simple_xml_dimension(&object, &key),
            _ => None,
        };
    }
    if let Some(mut value) = value {
        while let Value::Reference(reference) = value {
            value = reference.get();
        }
        Ok(Some(value))
    } else {
        Ok(None)
    }
}

pub(super) fn native_property_truthy(value: &Value) -> bool {
    match value {
        Value::Null | Value::Uninitialized | Value::Bool(false) => false,
        Value::Int(0) => false,
        Value::Float(value) if value.to_f64() == 0.0 => false,
        Value::String(value) if value.as_bytes().is_empty() || value.as_bytes() == b"0" => false,
        Value::Array(value) if value.is_empty() => false,
        Value::Reference(reference) => native_property_truthy(&reference.get()),
        Value::Object(object) if native_simple_xml_empty(object).is_some() => {
            !native_simple_xml_empty(object).unwrap_or(true)
        }
        _ => true,
    }
}

pub(super) fn native_property_is_set(value: &Value) -> bool {
    match value {
        Value::Null | Value::Uninitialized => false,
        Value::Reference(reference) => native_property_is_set(&reference.get()),
        _ => true,
    }
}

pub(super) fn unset_native_array_dims(value: &mut Value, keys: &[php_runtime::api::ArrayKey]) {
    if let Value::Reference(reference) = value {
        let mut target = reference.get();
        unset_native_array_dims(&mut target, keys);
        reference.set(target);
        return;
    }
    let Some((key, rest)) = keys.split_first() else {
        return;
    };
    let Value::Array(array) = value else {
        return;
    };
    if rest.is_empty() {
        array.remove(key);
    } else if let Some(mut nested) = array.get_mut(key) {
        unset_native_array_dims(&mut nested, rest);
    }
}

pub(super) fn assign_native_array_dims(
    value: &mut Value,
    keys: &[php_runtime::api::ArrayKey],
    replacement: Value,
    append: bool,
) {
    if let Value::Reference(reference) = value {
        let mut target = reference.get();
        assign_native_array_dims(&mut target, keys, replacement, append);
        reference.set(target);
        return;
    }
    if !matches!(value, Value::Array(_)) {
        *value = Value::Array(php_runtime::api::PhpArray::new());
    }
    let Value::Array(array) = value else {
        unreachable!("array value was initialized above")
    };
    let Some((key, rest)) = keys.split_first() else {
        if append {
            array.append(replacement);
        }
        return;
    };
    if rest.is_empty() && !append {
        if let Some(Value::Reference(reference)) = array.get(key).cloned() {
            reference.set(replacement);
        } else {
            array.insert(key.clone(), replacement);
        }
    } else {
        let mut nested = array.get(key).cloned().unwrap_or(Value::Null);
        assign_native_array_dims(&mut nested, rest, replacement, append);
        array.insert(key.clone(), nested);
    }
}

pub(super) fn native_coerce_call_argument(
    value: Value,
    type_: &php_ir::IrReturnType,
    strict: bool,
) -> Value {
    use php_ir::IrReturnType as Type;
    if let Value::Reference(reference) = &value {
        return Value::Reference(reference.clone());
    }
    if let Type::Nullable { inner } = type_ {
        if matches!(value, Value::Null) {
            return value;
        }
        return native_coerce_call_argument(value, inner, strict);
    }
    if matches!(type_, Type::Float)
        && let Value::Int(value) = value
    {
        return Value::Float(php_runtime::api::FloatValue::from_f64(value as f64));
    }
    if strict || native_value_matches_ir_type(&value, type_) {
        return value;
    }
    match (type_, value) {
        (Type::Int, Value::String(value)) => value
            .to_string_lossy()
            .trim()
            .parse::<i64>()
            .map(Value::Int)
            .unwrap_or(Value::String(value)),
        (Type::Int, Value::Float(value)) => Value::Int(value.to_f64() as i64),
        (Type::Int, Value::Bool(value)) => Value::Int(i64::from(value)),
        (Type::Float, Value::String(value)) => value
            .to_string_lossy()
            .trim()
            .parse::<f64>()
            .map(|value| Value::Float(php_runtime::api::FloatValue::from_f64(value)))
            .unwrap_or(Value::String(value)),
        (Type::Float, Value::Bool(value)) => {
            Value::Float(php_runtime::api::FloatValue::from_f64(if value {
                1.0
            } else {
                0.0
            }))
        }
        (Type::String, Value::Int(value)) => {
            Value::String(PhpString::from_bytes(value.to_string().into_bytes()))
        }
        (Type::String, Value::Float(value)) => Value::String(PhpString::from_bytes(
            value.to_f64().to_string().into_bytes(),
        )),
        (Type::String, Value::Bool(value)) => Value::String(PhpString::from_bytes(if value {
            b"1".to_vec()
        } else {
            Vec::new()
        })),
        (Type::Bool, value @ (Value::Int(_) | Value::Float(_) | Value::String(_))) => {
            Value::Bool(native_property_truthy(&value))
        }
        (Type::Nullable { inner }, value) => native_coerce_call_argument(value, inner, strict),
        (Type::Union { members }, value) => members
            .iter()
            .map(|member| native_coerce_call_argument(value.clone(), member, strict))
            .find(|candidate| native_value_matches_ir_type(candidate, type_))
            .unwrap_or(value),
        (_, value) => value,
    }
}

pub(super) fn native_value_with_owner_unit(value: Value, owner_unit: Option<usize>) -> Value {
    match value {
        Value::Callable(callable) => match callable.as_ref() {
            php_runtime::api::CallableValue::Closure(closure)
                if closure.context.owner_unit.is_none() && owner_unit.is_some() =>
            {
                Value::Callable(Box::new(php_runtime::api::CallableValue::Closure(
                    closure.clone().with_owner_unit(owner_unit),
                )))
            }
            _ => Value::Callable(callable),
        },
        value => value,
    }
}

pub(super) fn execute_baseline_instanceof(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    let (object, static_target) = match &instruction.kind {
        php_ir::InstructionKind::InstanceOf { class_name, .. } => {
            (arguments.first().copied(), Some(class_name.as_str()))
        }
        php_ir::InstructionKind::DynamicInstanceOf { .. } => (arguments.first().copied(), None),
        _ => return None,
    };
    let Some(object) = object else {
        return Some(Err("instanceof receiver is missing".to_owned()));
    };
    let target = if let Some(target) = static_target {
        target.to_owned()
    } else {
        let Some(target) = arguments.get(1) else {
            return Some(Err("instanceof target is missing".to_owned()));
        };
        let direct_target = context.dereference_direct_encoding(*target);
        if let Some(bytes) = context.native_string_name_bytes(direct_target) {
            String::from_utf8_lossy(&bytes).into_owned()
        } else if let Some(object) = context.native_query_object(direct_target) {
            object.class_name()
        } else {
            match context.decode_baseline_value(*target) {
                Ok(Value::String(value)) => value.to_string_lossy(),
                Ok(Value::Object(object)) => object.class_name(),
                Ok(value) => {
                    return Some(Err(format!(
                        "instanceof target must be a class name, {} given",
                        native_value_type_name(&value)
                    )));
                }
                Err(error) => return Some(Err(error)),
            }
        }
    };
    let direct_object = context.dereference_direct_encoding(object);
    let result = match context.native_encoded_value_kind(direct_object) {
        Some(NativeEncodedValueKind::Callable) => target.eq_ignore_ascii_case("Closure"),
        Some(NativeEncodedValueKind::Fiber) => target.eq_ignore_ascii_case("Fiber"),
        Some(NativeEncodedValueKind::Generator) => target.eq_ignore_ascii_case("Generator"),
        Some(NativeEncodedValueKind::Object) => {
            let Some(object) = context.native_query_object(direct_object) else {
                return Some(Err("instanceof receiver lost its native object".to_owned()));
            };
            native_internal_instanceof(&object.class_name(), &target)
                .unwrap_or_else(|| native_class_is_a(context, &object.class_name(), &target))
        }
        _ => match context.decode_baseline_value(object) {
            Ok(Value::Object(object)) => native_internal_instanceof(&object.class_name(), &target)
                .unwrap_or_else(|| native_class_is_a(context, &object.class_name(), &target)),
            Ok(Value::Callable(_)) => target.eq_ignore_ascii_case("Closure"),
            Ok(Value::Fiber(_)) => target.eq_ignore_ascii_case("Fiber"),
            Ok(Value::Generator(_)) => target.eq_ignore_ascii_case("Generator"),
            Ok(Value::Array(array)) => crate::vm::native_exception_fields(Value::Array(array))
                .is_some_and(|(class, _, _)| {
                    let normalized = class.to_ascii_lowercase();
                    target.eq_ignore_ascii_case(&class)
                        || target.eq_ignore_ascii_case("Throwable")
                        || (target.eq_ignore_ascii_case("Exception")
                            && normalized.ends_with("exception"))
                        || (target.eq_ignore_ascii_case("Error") && normalized.ends_with("error"))
                }),
            Ok(Value::Reference(reference)) => match reference.get() {
                Value::Object(object) => native_internal_instanceof(&object.class_name(), &target)
                    .unwrap_or_else(|| native_class_is_a(context, &object.class_name(), &target)),
                Value::Callable(_) => target.eq_ignore_ascii_case("Closure"),
                Value::Fiber(_) => target.eq_ignore_ascii_case("Fiber"),
                Value::Generator(_) => target.eq_ignore_ascii_case("Generator"),
                _ => false,
            },
            Ok(_) => false,
            Err(error) => return Some(Err(error)),
        },
    };
    Some(context.encode_baseline_value(Value::Bool(result)))
}
