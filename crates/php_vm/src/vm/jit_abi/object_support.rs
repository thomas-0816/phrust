use super::*;

pub(super) fn native_metadata_object(
    class_name: &str,
    properties: impl IntoIterator<Item = (String, Value)>,
) -> php_runtime::api::ObjectRef {
    let class = php_runtime::api::ClassEntry {
        name: std::sync::Arc::from(normalize_class_name(class_name)),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: php_runtime::api::ClassFlags::default(),
    };
    let object = php_runtime::api::ObjectRef::new_with_display_name(&class, class_name);
    for (name, value) in properties {
        object.set_property(name, value);
    }
    object
}

pub(super) fn native_reflection_type(type_: &php_ir::IrReturnType) -> Value {
    Value::Object(native_metadata_object(
        "ReflectionNamedType",
        [(
            "name".to_owned(),
            Value::String(PhpString::from_bytes(
                native_ir_type_name(type_).into_bytes(),
            )),
        )],
    ))
}

pub(super) fn native_reflection_attributes(
    context: &NativeExecutionContext<'_>,
    attributes: &[php_ir::AttributeEntry],
) -> Result<Value, String> {
    let mut result = php_runtime::api::PhpArray::new();
    for attribute in attributes {
        let name = attribute.name.trim_start_matches('\\').to_owned();
        let arguments = attribute
            .arguments
            .iter()
            .map(|argument| {
                context
                    .unit
                    .constants
                    .get(argument.index())
                    .ok_or_else(|| "reflection attribute argument is missing".to_owned())
                    .and_then(ir_constant_value)
            })
            .collect::<Result<Vec<_>, _>>()?;
        result.append(Value::Object(native_metadata_object(
            "ReflectionAttribute",
            [
                (
                    "name".to_owned(),
                    Value::String(PhpString::from_bytes(name.into_bytes())),
                ),
                (
                    "arguments".to_owned(),
                    Value::Array(php_runtime::api::PhpArray::from_packed(arguments)),
                ),
            ],
        )));
    }
    Ok(Value::Array(result))
}

pub(super) fn native_reflection_parameter(
    context: &NativeExecutionContext<'_>,
    parameter: &php_ir::IrParam,
) -> Result<Value, String> {
    let default = parameter
        .default
        .as_ref()
        .map(ir_constant_value)
        .transpose()?
        .unwrap_or(Value::Null);
    Ok(Value::Object(native_metadata_object(
        "ReflectionParameter",
        [
            (
                "name".to_owned(),
                Value::String(PhpString::from_bytes(parameter.name.as_bytes().to_vec())),
            ),
            (
                "type".to_owned(),
                parameter
                    .type_
                    .as_ref()
                    .map_or(Value::Null, native_reflection_type),
            ),
            ("optional".to_owned(), Value::Bool(!parameter.required)),
            ("default".to_owned(), default),
            (
                "attributes".to_owned(),
                native_reflection_attributes(context, &parameter.attributes)?,
            ),
        ],
    )))
}

pub(super) fn native_reflection_function_properties(
    context: &NativeExecutionContext<'_>,
    function: &php_ir::IrFunction,
    closure: Option<&php_runtime::api::ClosurePayload>,
) -> Result<Vec<(String, Value)>, String> {
    let parameters = function
        .params
        .iter()
        .map(|parameter| native_reflection_parameter(context, parameter))
        .collect::<Result<Vec<_>, _>>()?;
    let mut statics = php_runtime::api::PhpArray::new();
    if let Some(closure) = closure {
        for (capture, value) in function.captures.iter().zip(&closure.captures) {
            statics.insert(
                php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                    capture.name.as_bytes().to_vec(),
                )),
                value.value().cloned().unwrap_or(Value::Null),
            );
        }
    }
    Ok(vec![
        (
            "name".to_owned(),
            Value::String(PhpString::from_bytes(function.name.as_bytes().to_vec())),
        ),
        (
            "return_type".to_owned(),
            function
                .return_type
                .as_ref()
                .map_or(Value::Null, native_reflection_type),
        ),
        (
            "parameters".to_owned(),
            Value::Array(php_runtime::api::PhpArray::from_packed(parameters)),
        ),
        (
            "required".to_owned(),
            Value::Int(function.params.iter().filter(|p| p.required).count() as i64),
        ),
        (
            "is_closure".to_owned(),
            Value::Bool(function.flags.is_closure),
        ),
        ("internal".to_owned(), Value::Bool(false)),
        (
            "file_name".to_owned(),
            context
                .unit
                .files
                .get(function.span.file.index())
                .map_or(Value::Bool(false), |file| {
                    Value::String(PhpString::from_bytes(file.path.as_bytes().to_vec()))
                }),
        ),
        ("static_variables".to_owned(), Value::Array(statics)),
        (
            "attributes".to_owned(),
            native_reflection_attributes(context, &function.attributes)?,
        ),
    ])
}

pub(super) fn execute_native_array_object(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    if let php_ir::InstructionKind::NewObject { class_name, .. } = &instruction.kind
        && let Some(result) = construct_native_spl_iterator(context, class_name, arguments)
    {
        return Some(result);
    }
    match &instruction.kind {
        php_ir::InstructionKind::NewObject {
            class_name,
            display_class_name,
            ..
        } if class_name.eq_ignore_ascii_case("stdClass") => {
            let class = php_runtime::api::ClassEntry {
                name: std::sync::Arc::from("stdclass"),
                parent: None,
                interfaces: Vec::new(),
                methods: Vec::new(),
                properties: Vec::new(),
                constants: Vec::new(),
                enum_cases: Vec::new(),
                attributes: Vec::new(),
                enum_backing_type: None,
                constructor_id: None,
                flags: php_runtime::api::ClassFlags::default(),
            };
            Some(context.encode(Value::Object(
                php_runtime::api::ObjectRef::new_with_display_name(
                    &class,
                    display_class_name.clone(),
                ),
            )))
        }
        php_ir::InstructionKind::NewObject {
            class_name,
            display_class_name,
            ..
        } if class_name.eq_ignore_ascii_case("ArrayObject") => {
            let class = php_runtime::api::ClassEntry {
                name: std::sync::Arc::from("arrayobject"),
                parent: None,
                interfaces: Vec::new(),
                methods: Vec::new(),
                properties: Vec::new(),
                constants: Vec::new(),
                enum_cases: Vec::new(),
                attributes: Vec::new(),
                enum_backing_type: None,
                constructor_id: None,
                flags: php_runtime::api::ClassFlags::default(),
            };
            let object = php_runtime::api::ObjectRef::new_with_display_name(
                &class,
                display_class_name.clone(),
            );
            if let Some(array) = arguments.first()
                && let Ok(Value::Array(array)) = context.decode(*array)
            {
                for (key, value) in array.iter() {
                    let name = match key {
                        php_runtime::api::ArrayKey::Int(key) => key.to_string(),
                        php_runtime::api::ArrayKey::String(key) => key.to_string_lossy(),
                    };
                    object.set_property(name, value.clone());
                }
            }
            Some(context.encode(Value::Object(object)))
        }
        php_ir::InstructionKind::NewObject {
            class_name,
            display_class_name,
            ..
        } if class_name.eq_ignore_ascii_case("ReflectionEnum")
            || class_name.eq_ignore_ascii_case("ReflectionClass")
            || class_name.eq_ignore_ascii_case("ReflectionFunction")
            || class_name.eq_ignore_ascii_case("ReflectionMethod")
            || class_name.eq_ignore_ascii_case("ReflectionProperty")
            || class_name.eq_ignore_ascii_case("ReflectionEnumUnitCase")
            || class_name.eq_ignore_ascii_case("ReflectionEnumBackedCase") =>
        {
            let result = (|| -> Result<i64, String> {
                if class_name.eq_ignore_ascii_case("ReflectionFunction") {
                    let argument = arguments
                        .first()
                        .ok_or_else(|| "ReflectionFunction expects a function".to_owned())?;
                    let value = match context.decode(*argument)? {
                        Value::Reference(reference) => reference.get(),
                        value => value,
                    };
                    let (function_id, closure) = match &value {
                        Value::String(name) => (context.function_id(&name.to_string_lossy()), None),
                        Value::Callable(callable) => match callable.as_ref() {
                            php_runtime::api::CallableValue::UserFunction { name } => {
                                (context.function_id(name), None)
                            }
                            php_runtime::api::CallableValue::Closure(closure) => (
                                Some(php_ir::FunctionId::new(closure.function)),
                                Some(closure.clone()),
                            ),
                            _ => (None, None),
                        },
                        _ => (None, None),
                    };
                    if function_id.is_none()
                        && let Value::String(name) = &value
                        && let Some(metadata) =
                            php_std::arginfo::function_metadata_indexed(&name.to_string_lossy())
                    {
                        let parameters = metadata
                            .params
                            .iter()
                            .map(|parameter| {
                                let default = parameter
                                    .default_value
                                    .map(|value| native_builtin_default_value(context, value))
                                    .transpose()?
                                    .unwrap_or(Value::Null);
                                Ok::<Value, String>(Value::Object(native_metadata_object(
                                    "ReflectionParameter",
                                    [
                                        (
                                            "name".to_owned(),
                                            Value::String(PhpString::from_bytes(
                                                parameter.name.as_bytes().to_vec(),
                                            )),
                                        ),
                                        ("optional".to_owned(), Value::Bool(parameter.optional)),
                                        ("default".to_owned(), default),
                                        ("by_ref".to_owned(), Value::Bool(parameter.by_ref)),
                                    ],
                                )))
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        let required = metadata
                            .params
                            .iter()
                            .filter(|parameter| !parameter.optional && !parameter.variadic)
                            .count();
                        return context.encode(Value::Object(native_metadata_object(
                            display_class_name,
                            [
                                (
                                    "name".to_owned(),
                                    Value::String(PhpString::from_bytes(
                                        metadata.name.as_bytes().to_vec(),
                                    )),
                                ),
                                (
                                    "parameters".to_owned(),
                                    Value::Array(php_runtime::api::PhpArray::from_packed(
                                        parameters,
                                    )),
                                ),
                                ("required".to_owned(), Value::Int(required as i64)),
                                ("is_closure".to_owned(), Value::Bool(false)),
                                ("internal".to_owned(), Value::Bool(true)),
                                ("file_name".to_owned(), Value::Bool(false)),
                                (
                                    "static_variables".to_owned(),
                                    Value::Array(php_runtime::api::PhpArray::new()),
                                ),
                            ],
                        )));
                    }
                    let function_id = function_id
                        .ok_or_else(|| "ReflectionFunction target is missing".to_owned())?;
                    let function = context
                        .unit
                        .functions
                        .get(function_id.index())
                        .ok_or_else(|| "ReflectionFunction metadata is missing".to_owned())?;
                    let properties =
                        native_reflection_function_properties(context, function, closure.as_ref())?;
                    return context.encode(Value::Object(native_metadata_object(
                        display_class_name,
                        properties,
                    )));
                }
                if class_name.eq_ignore_ascii_case("ReflectionMethod") {
                    if arguments.len() != 2 {
                        return Err(format!(
                            "E_PHP_THROW:ArgumentCountError:ReflectionMethod::__construct() expects exactly 2 arguments, {} given",
                            arguments.len()
                        ));
                    }
                    let owner = match context.decode(arguments[0])? {
                        Value::Reference(reference) => reference.get(),
                        value => value,
                    };
                    let owner = match owner {
                        Value::String(owner) => owner.to_string_lossy(),
                        Value::Object(owner) => owner.display_name().to_owned(),
                        _ => {
                            return Err(
                                "E_PHP_THROW:TypeError:ReflectionMethod::__construct(): Argument #1 ($objectOrMethod) must be of type object|string"
                                    .to_owned(),
                            );
                        }
                    };
                    let method = match context.decode(arguments[1])? {
                        Value::Reference(reference) => reference.get(),
                        value => value,
                    };
                    let Value::String(method) = method else {
                        return Err(
                            "E_PHP_THROW:TypeError:ReflectionMethod::__construct(): Argument #2 ($method) must be of type string"
                                .to_owned(),
                        );
                    };
                    let method = method.to_string_lossy();
                    if let Some(class) = context
                        .unit
                        .classes
                        .iter()
                        .find(|class| class.name == normalize_class_name(&owner))
                        && let Some(entry) = class
                            .methods
                            .iter()
                            .find(|entry| entry.name.eq_ignore_ascii_case(&method))
                    {
                        let function = &context.unit.functions[entry.function.index()];
                        let mut properties =
                            native_reflection_function_properties(context, function, None)?;
                        properties.retain(|(name, _)| name != "name");
                        properties.extend([
                            (
                                "name".to_owned(),
                                Value::String(PhpString::from_bytes(
                                    entry.name.as_bytes().to_vec(),
                                )),
                            ),
                            (
                                "declaring_class".to_owned(),
                                Value::String(PhpString::from_bytes(
                                    class.display_name.as_bytes().to_vec(),
                                )),
                            ),
                            ("static".to_owned(), Value::Bool(entry.flags.is_static)),
                            ("internal".to_owned(), Value::Bool(false)),
                        ]);
                        return context.encode(Value::Object(native_metadata_object(
                            display_class_name,
                            properties,
                        )));
                    }
                    if php_std::generated::arginfo::method_metadata(&owner, &method).is_some() {
                        return context.encode(Value::Object(native_metadata_object(
                            display_class_name,
                            [
                                (
                                    "name".to_owned(),
                                    Value::String(PhpString::from_bytes(method.into_bytes())),
                                ),
                                (
                                    "declaring_class".to_owned(),
                                    Value::String(PhpString::from_bytes(owner.into_bytes())),
                                ),
                                ("internal".to_owned(), Value::Bool(true)),
                            ],
                        )));
                    }
                    return Err(format!(
                        "E_PHP_THROW:ReflectionException:Method {owner}::{method}() does not exist"
                    ));
                }
                if class_name.eq_ignore_ascii_case("ReflectionProperty") {
                    if arguments.len() != 2 {
                        return Err(format!(
                            "E_PHP_THROW:ArgumentCountError:ReflectionProperty::__construct() expects exactly 2 arguments, {} given",
                            arguments.len()
                        ));
                    }
                    let owner = match context.decode(arguments[0])? {
                        Value::Reference(reference) => reference.get(),
                        value => value,
                    };
                    let owner = match owner {
                        Value::String(owner) => owner.to_string_lossy(),
                        Value::Object(owner) => owner.display_name().to_owned(),
                        _ => {
                            return Err(
                                "E_PHP_THROW:TypeError:ReflectionProperty::__construct(): Argument #1 ($class) must be of type object|string"
                                    .to_owned(),
                            );
                        }
                    };
                    let member = match context.decode(arguments[1])? {
                        Value::Reference(reference) => reference.get(),
                        value => value,
                    };
                    let Value::String(member) = member else {
                        return Err(
                            "E_PHP_THROW:TypeError:ReflectionProperty::__construct(): Argument #2 ($property) must be of type string"
                                .to_owned(),
                        );
                    };
                    let member = member.to_string_lossy();
                    let class = context
                        .unit
                        .classes
                        .iter()
                        .find(|class| class.name == normalize_class_name(&owner))
                        .cloned();
                    let Some((class, property)) = class.and_then(|class| {
                        class
                            .properties
                            .iter()
                            .find(|property| property.name == member)
                            .cloned()
                            .map(|property| (class, property))
                    }) else {
                        return Err(format!(
                            "E_PHP_THROW:ReflectionException:Property {owner}::${member} does not exist"
                        ));
                    };
                    let default = property
                        .default
                        .and_then(|id| context.unit.constants.get(id.index()))
                        .map(ir_constant_value)
                        .transpose()?
                        .unwrap_or(Value::Null);
                    return context.encode(Value::Object(native_metadata_object(
                        display_class_name,
                        [
                            (
                                "name".to_owned(),
                                Value::String(PhpString::from_bytes(
                                    property.name.as_bytes().to_vec(),
                                )),
                            ),
                            (
                                "declaring_class".to_owned(),
                                Value::String(PhpString::from_bytes(
                                    class.display_name.as_bytes().to_vec(),
                                )),
                            ),
                            (
                                "type".to_owned(),
                                property
                                    .type_
                                    .as_ref()
                                    .map_or(Value::Null, native_reflection_type),
                            ),
                            ("default".to_owned(), default),
                            ("private".to_owned(), Value::Bool(property.flags.is_private)),
                            (
                                "attributes".to_owned(),
                                native_reflection_attributes(context, &property.attributes)?,
                            ),
                        ],
                    )));
                }
                let name = arguments
                    .first()
                    .map(|value| context.decode(*value))
                    .transpose()?
                    .and_then(|value| match value {
                        Value::String(name) => Some(name.to_string_lossy()),
                        _ => None,
                    })
                    .unwrap_or_default();
                if class_name.eq_ignore_ascii_case("ReflectionClass")
                    || class_name.eq_ignore_ascii_case("ReflectionEnum")
                {
                    native_autoload_class(context, &name, instruction)?;
                }
                let mut properties = vec![(
                    "name".to_owned(),
                    Value::String(PhpString::from_bytes(name.as_bytes().to_vec())),
                )];
                if class_name.to_ascii_lowercase().contains("case") {
                    let case = arguments
                        .get(1)
                        .map(|value| context.decode(*value))
                        .transpose()?
                        .and_then(|value| match value {
                            Value::String(name) => Some(name.to_string_lossy()),
                            _ => None,
                        })
                        .unwrap_or_default();
                    properties[0].1 = Value::String(PhpString::from_bytes(case.into_bytes()));
                    properties.push((
                        "enum".to_owned(),
                        Value::String(PhpString::from_bytes(name.into_bytes())),
                    ));
                }
                context.encode(Value::Object(native_metadata_object(
                    display_class_name,
                    properties,
                )))
            })();
            Some(result)
        }
        php_ir::InstructionKind::CallMethod { method, .. }
            if method.eq_ignore_ascii_case("getArrayCopy") =>
        {
            let Some(object) = arguments.first() else {
                return Some(Err("ArrayObject receiver is missing".to_owned()));
            };
            let Ok(Value::Object(object)) = context.decode(*object) else {
                return Some(Err("ArrayObject receiver is invalid".to_owned()));
            };
            if !object.class_name().eq_ignore_ascii_case("arrayobject") {
                return None;
            }
            let mut array = php_runtime::api::PhpArray::new();
            for (name, value) in object.properties_snapshot() {
                array.insert(
                    php_runtime::api::ArrayKey::String(PhpString::from_bytes(name.into_bytes())),
                    value,
                );
            }
            Some(context.encode(Value::Array(array)))
        }
        php_ir::InstructionKind::CallMethod { method, .. } => {
            let receiver = arguments.first().copied()?;
            let Ok(Value::Object(object)) = context.decode(receiver) else {
                return None;
            };
            if !object
                .class_name()
                .to_ascii_lowercase()
                .starts_with("reflection")
            {
                return None;
            }
            let result = (|| -> Result<i64, String> {
                let property = |name: &str| object.get_property(name).unwrap_or(Value::Null);
                let string_property = |name: &str| match property(name) {
                    Value::String(value) => Some(value.to_string_lossy()),
                    _ => None,
                };
                let class_name = object.class_name().to_ascii_lowercase();
                match method.to_ascii_lowercase().as_str() {
                    "getname" => context.encode(property("name")),
                    "getarguments" => context.encode(property("arguments")),
                    "getattributes" => {
                        if !matches!(property("attributes"), Value::Null) {
                            return context.encode(property("attributes"));
                        }
                        let attributes =
                            if class_name == "reflectionclass" || class_name == "reflectionenum" {
                                string_property("name")
                                    .and_then(|name| {
                                        context
                                            .unit
                                            .classes
                                            .iter()
                                            .find(|class| class.name == normalize_class_name(&name))
                                    })
                                    .map(|class| class.attributes.as_slice())
                            } else if class_name.contains("enum") && class_name.contains("case") {
                                let enum_name = string_property("enum").unwrap_or_default();
                                let case_name = string_property("name").unwrap_or_default();
                                context
                                    .unit
                                    .classes
                                    .iter()
                                    .find(|class| class.name == normalize_class_name(&enum_name))
                                    .and_then(|class| {
                                        class.enum_cases.iter().find(|case| case.name == case_name)
                                    })
                                    .map(|case| case.attributes.as_slice())
                            } else {
                                None
                            };
                        context.encode(match attributes {
                            Some(attributes) => native_reflection_attributes(context, attributes)?,
                            None => Value::Array(php_runtime::api::PhpArray::new()),
                        })
                    }
                    "getreturntype" => context.encode(property("return_type")),
                    "gettype" => context.encode(property("type")),
                    "getparameters" => context.encode(property("parameters")),
                    "getnumberofparameters" => match property("parameters") {
                        Value::Array(parameters) => {
                            context.encode(Value::Int(parameters.len() as i64))
                        }
                        _ => context.encode(Value::Int(0)),
                    },
                    "getnumberofrequiredparameters" => context.encode(property("required")),
                    "isinternal" => context.encode(property("internal")),
                    "getfilename" => context.encode(property("file_name")),
                    "isclosure" => context.encode(property("is_closure")),
                    "getstaticvariables" => context.encode(property("static_variables")),
                    "getclosurescopeclass" => context.encode(Value::Null),
                    "isbuiltin" => context.encode(Value::Bool(true)),
                    "isoptional" => context.encode(property("optional")),
                    "ispassedbyreference" => context.encode(property("by_ref")),
                    "getdefaultvalue" => context.encode(property("default")),
                    "isstatic" => context.encode(property("static")),
                    "isprivate" => context.encode(property("private")),
                    "isprotected" => context.encode(property("protected")),
                    "getvalue" => context.encode(property("value")),
                    "getbackingvalue" => context.encode(property("value")),
                    "getdoccomment" => context.encode(Value::Bool(false)),
                    "getdeclaringclass" => {
                        let name = string_property("declaring_class").unwrap_or_default();
                        context.encode(Value::Object(native_metadata_object(
                            "ReflectionClass",
                            [(
                                "name".to_owned(),
                                Value::String(PhpString::from_bytes(name.into_bytes())),
                            )],
                        )))
                    }
                    "isfinal" | "isinterface" | "isinstantiable" | "isbacked" => {
                        let name = string_property("name").unwrap_or_default();
                        let class = context
                            .unit
                            .classes
                            .iter()
                            .find(|class| class.name == normalize_class_name(&name));
                        let value =
                            class.is_some_and(|class| match method.to_ascii_lowercase().as_str() {
                                "isfinal" => class.flags.is_final,
                                "isinterface" => class.flags.is_interface,
                                "isinstantiable" => {
                                    !class.flags.is_interface
                                        && !class.flags.is_trait
                                        && !class.flags.is_abstract
                                        && !class.flags.is_enum
                                }
                                "isbacked" => class.enum_backing_type.is_some(),
                                _ => false,
                            });
                        context.encode(Value::Bool(value))
                    }
                    "getbackingtype" => {
                        let name = string_property("name").unwrap_or_default();
                        let type_name = context
                            .unit
                            .classes
                            .iter()
                            .find(|class| class.name == normalize_class_name(&name))
                            .and_then(|class| class.enum_backing_type)
                            .map(|type_| match type_ {
                                php_ir::ClassEnumBackingType::Int => "int",
                                php_ir::ClassEnumBackingType::String => "string",
                            })
                            .unwrap_or("mixed");
                        context.encode(native_reflection_type(&match type_name {
                            "int" => php_ir::IrReturnType::Int,
                            "string" => php_ir::IrReturnType::String,
                            _ => php_ir::IrReturnType::Mixed,
                        }))
                    }
                    "getinterfacenames" => {
                        let name = string_property("name").unwrap_or_default();
                        let names = context
                            .unit
                            .classes
                            .iter()
                            .find(|class| class.name == normalize_class_name(&name))
                            .map(|class| {
                                class
                                    .interfaces
                                    .iter()
                                    .map(|name| {
                                        let display = context
                                            .unit
                                            .classes
                                            .iter()
                                            .find(|candidate| {
                                                candidate.name == normalize_class_name(name)
                                            })
                                            .map_or(name.as_str(), |candidate| {
                                                candidate.display_name.as_str()
                                            });
                                        Value::String(PhpString::from_bytes(
                                            display.as_bytes().to_vec(),
                                        ))
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        context.encode(Value::Array(php_runtime::api::PhpArray::from_packed(names)))
                    }
                    "getmethods" | "getmethod" => {
                        let owner = string_property("name").unwrap_or_default();
                        let class = context
                            .unit
                            .classes
                            .iter()
                            .find(|class| class.name == normalize_class_name(&owner))
                            .cloned()
                            .ok_or_else(|| "reflection class metadata is missing".to_owned())?;
                        let requested = arguments
                            .get(1)
                            .map(|value| context.decode(*value))
                            .transpose()?
                            .and_then(|value| match value {
                                Value::String(value) => Some(value.to_string_lossy()),
                                _ => None,
                            });
                        let methods = class
                            .methods
                            .iter()
                            .filter(|entry| {
                                requested
                                    .as_ref()
                                    .is_none_or(|name| entry.name.eq_ignore_ascii_case(name))
                            })
                            .map(|entry| {
                                let function = &context.unit.functions[entry.function.index()];
                                let mut properties =
                                    native_reflection_function_properties(context, function, None)?;
                                properties
                                    .retain(|(name, _)| name != "name" && name != "attributes");
                                properties.extend([
                                    (
                                        "name".to_owned(),
                                        Value::String(PhpString::from_bytes(
                                            entry.name.as_bytes().to_vec(),
                                        )),
                                    ),
                                    (
                                        "declaring_class".to_owned(),
                                        Value::String(PhpString::from_bytes(
                                            class.display_name.as_bytes().to_vec(),
                                        )),
                                    ),
                                    ("static".to_owned(), Value::Bool(entry.flags.is_static)),
                                    (
                                        "attributes".to_owned(),
                                        native_reflection_attributes(context, &entry.attributes)?,
                                    ),
                                ]);
                                Ok::<Value, String>(Value::Object(native_metadata_object(
                                    "ReflectionMethod",
                                    properties,
                                )))
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        if requested.is_some() {
                            context.encode(methods.into_iter().next().unwrap_or(Value::Null))
                        } else {
                            context.encode(Value::Array(php_runtime::api::PhpArray::from_packed(
                                methods,
                            )))
                        }
                    }
                    "getproperties" | "getproperty" => {
                        let owner = string_property("name").unwrap_or_default();
                        let class = context
                            .unit
                            .classes
                            .iter()
                            .find(|class| class.name == normalize_class_name(&owner))
                            .cloned()
                            .ok_or_else(|| "reflection class metadata is missing".to_owned())?;
                        let requested = arguments
                            .get(1)
                            .map(|value| context.decode(*value))
                            .transpose()?
                            .and_then(|value| match value {
                                Value::String(value) => Some(value.to_string_lossy()),
                                _ => None,
                            });
                        let properties = class
                            .properties
                            .iter()
                            .filter(|entry| {
                                requested.as_ref().is_none_or(|name| entry.name == *name)
                            })
                            .map(|entry| {
                                let default = entry
                                    .default
                                    .and_then(|id| context.unit.constants.get(id.index()))
                                    .map(ir_constant_value)
                                    .transpose()?
                                    .unwrap_or(Value::Null);
                                Ok::<Value, String>(Value::Object(native_metadata_object(
                                    "ReflectionProperty",
                                    [
                                        (
                                            "name".to_owned(),
                                            Value::String(PhpString::from_bytes(
                                                entry.name.as_bytes().to_vec(),
                                            )),
                                        ),
                                        (
                                            "type".to_owned(),
                                            entry
                                                .type_
                                                .as_ref()
                                                .map_or(Value::Null, native_reflection_type),
                                        ),
                                        ("default".to_owned(), default),
                                        ("private".to_owned(), Value::Bool(entry.flags.is_private)),
                                        (
                                            "attributes".to_owned(),
                                            native_reflection_attributes(
                                                context,
                                                &entry.attributes,
                                            )?,
                                        ),
                                    ],
                                )))
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        if requested.is_some() {
                            context.encode(properties.into_iter().next().unwrap_or(Value::Null))
                        } else {
                            context.encode(Value::Array(php_runtime::api::PhpArray::from_packed(
                                properties,
                            )))
                        }
                    }
                    "getconstants" | "getconstant" | "getreflectionconstant" => {
                        let owner = string_property("name").unwrap_or_default();
                        let local_class = context
                            .unit
                            .classes
                            .iter()
                            .find(|class| class.name == normalize_class_name(&owner))
                            .cloned();
                        let (owner_unit, class) = if let Some(class) = local_class {
                            (None, class)
                        } else {
                            let (unit, class) = native_external_class(context, &owner)
                                .ok_or_else(|| "reflection class metadata is missing".to_owned())?;
                            (Some(unit), class)
                        };
                        let requested = arguments
                            .get(1)
                            .map(|value| context.decode(*value))
                            .transpose()?
                            .and_then(|value| match value {
                                Value::String(value) => Some(value.to_string_lossy()),
                                _ => None,
                            });
                        if method.eq_ignore_ascii_case("getConstants") {
                            let mut values = php_runtime::api::PhpArray::new();
                            for entry in &class.constants {
                                let value = entry
                                    .value
                                    .and_then(|id| {
                                        owner_unit.map_or_else(
                                            || context.unit.constants.get(id.index()),
                                            |unit| {
                                                context.dynamic_units.get(unit).and_then(
                                                    |package| {
                                                        package
                                                            .compiled
                                                            .unit()
                                                            .constants
                                                            .get(id.index())
                                                    },
                                                )
                                            },
                                        )
                                    })
                                    .map(ir_constant_value)
                                    .transpose()?
                                    .unwrap_or(Value::Null);
                                values.insert(
                                    php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                                        entry.name.as_bytes().to_vec(),
                                    )),
                                    value,
                                );
                            }
                            return context.encode(Value::Array(values));
                        }
                        let entry = class.constants.iter().find(|entry| {
                            requested.as_ref().is_some_and(|name| entry.name == *name)
                        });
                        let value = entry
                            .and_then(|entry| entry.value)
                            .and_then(|id| {
                                owner_unit.map_or_else(
                                    || context.unit.constants.get(id.index()),
                                    |unit| {
                                        context.dynamic_units.get(unit).and_then(|package| {
                                            package.compiled.unit().constants.get(id.index())
                                        })
                                    },
                                )
                            })
                            .map(ir_constant_value)
                            .transpose()?
                            .unwrap_or(Value::Null);
                        if method.eq_ignore_ascii_case("getConstant") {
                            return context.encode(value);
                        }
                        let entry =
                            entry.ok_or_else(|| "reflection constant is missing".to_owned())?;
                        context.encode(Value::Object(native_metadata_object(
                            "ReflectionClassConstant",
                            [
                                (
                                    "name".to_owned(),
                                    Value::String(PhpString::from_bytes(
                                        entry.name.as_bytes().to_vec(),
                                    )),
                                ),
                                ("value".to_owned(), value),
                                (
                                    "protected".to_owned(),
                                    Value::Bool(entry.flags.is_protected),
                                ),
                                (
                                    "attributes".to_owned(),
                                    native_reflection_attributes(context, &entry.attributes)?,
                                ),
                            ],
                        )))
                    }
                    "getcases" => {
                        let owner = string_property("name").unwrap_or_default();
                        let cases = context
                            .unit
                            .classes
                            .iter()
                            .find(|class| class.name == normalize_class_name(&owner))
                            .map(|class| {
                                class
                                    .enum_cases
                                    .iter()
                                    .map(|case| {
                                        let value = case
                                            .value
                                            .and_then(|id| context.unit.constants.get(id.index()))
                                            .map(ir_constant_value)
                                            .transpose()?
                                            .unwrap_or(Value::Null);
                                        Ok::<Value, String>(Value::Object(native_metadata_object(
                                            if case.value.is_some() {
                                                "ReflectionEnumBackedCase"
                                            } else {
                                                "ReflectionEnumUnitCase"
                                            },
                                            [
                                                (
                                                    "name".to_owned(),
                                                    Value::String(PhpString::from_bytes(
                                                        case.name.as_bytes().to_vec(),
                                                    )),
                                                ),
                                                (
                                                    "enum".to_owned(),
                                                    Value::String(PhpString::from_bytes(
                                                        owner.as_bytes().to_vec(),
                                                    )),
                                                ),
                                                ("value".to_owned(), value),
                                                (
                                                    "attributes".to_owned(),
                                                    native_reflection_attributes(
                                                        context,
                                                        &case.attributes,
                                                    )?,
                                                ),
                                            ],
                                        )))
                                    })
                                    .collect::<Result<Vec<_>, _>>()
                            })
                            .transpose()?
                            .unwrap_or_default();
                        context.encode(Value::Array(php_runtime::api::PhpArray::from_packed(cases)))
                    }
                    "hasmethod" => {
                        let owner = string_property("name").unwrap_or_default();
                        let requested = arguments
                            .get(1)
                            .map(|value| context.decode(*value))
                            .transpose()?
                            .and_then(|value| match value {
                                Value::String(value) => Some(value.to_string_lossy()),
                                _ => None,
                            })
                            .unwrap_or_default();
                        let exists = context
                            .unit
                            .classes
                            .iter()
                            .find(|class| class.name == normalize_class_name(&owner))
                            .is_some_and(|class| {
                                class
                                    .methods
                                    .iter()
                                    .any(|entry| entry.name.eq_ignore_ascii_case(&requested))
                            })
                            || native_external_method(context, &owner, &requested).is_some();
                        context.encode(Value::Bool(exists))
                    }
                    "newinstance" => {
                        let name = string_property("name").unwrap_or_default();
                        context.encode(Value::Object(native_metadata_object(&name, [])))
                    }
                    _ => Err(format!("native reflection method {method} is unsupported")),
                }
            })();
            Some(result)
        }
        _ => None,
    }
}
