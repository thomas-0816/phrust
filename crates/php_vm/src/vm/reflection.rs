//! Runtime implementation of the Reflection* built-in classes, extracted from the VM module.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::result_large_err)]

use super::prelude::*;

pub(super) fn reflection_runtime_class(name: &str) -> RuntimeClassEntry {
    RuntimeClassEntry {
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
        flags: RuntimeClassFlags::default(),
    }
}

pub(super) fn reflection_object(name: &str, properties: Vec<(&str, Value)>) -> ObjectRef {
    let class = reflection_runtime_class(name);
    let object = ObjectRef::new_with_display_name(&class, php_ir::display_class_name(name));
    for (property, value) in properties {
        object.set_property(property, value);
    }
    object
}

pub(super) fn reflection_string_arg(
    args: &[Value],
    index: usize,
    owner: &str,
) -> Result<String, String> {
    let Some(value) = args.get(index) else {
        return Err(format!(
            "E_PHP_VM_REFLECTION_ARITY: {owner} missing argument {index}"
        ));
    };
    Ok(to_string(value)?.to_string_lossy())
}

pub(super) fn reflection_value_string(value: &Value) -> Option<String> {
    to_string(value).ok().map(|value| value.to_string_lossy())
}

pub(super) fn reflection_string(value: impl AsRef<str>) -> Value {
    Value::String(PhpString::from_test_str(value.as_ref()))
}

pub(super) fn reflection_string_array(values: impl IntoIterator<Item = String>) -> Value {
    let mut array = PhpArray::new();
    for value in values {
        array.append(reflection_string(value));
    }
    Value::Array(array)
}

pub(super) fn reflection_objects_array(values: impl IntoIterator<Item = ObjectRef>) -> Value {
    let mut array = PhpArray::new();
    for value in values {
        array.append(Value::Object(value));
    }
    Value::Array(array)
}

pub(super) fn reflection_assoc_insert(array: &mut PhpArray, key: &str, value: Value) {
    array.insert(ArrayKey::String(PhpString::from_test_str(key)), value);
}

pub(super) fn reflection_type_value(type_: Option<&IrReturnType>) -> Value {
    let Some(type_) = type_ else {
        return Value::Null;
    };
    Value::Object(reflection_object(
        "ReflectionNamedType",
        vec![
            ("name", reflection_string(ir_type_name(type_))),
            ("allows_null", Value::Bool(ir_type_allows_null(type_))),
            ("builtin", Value::Bool(ir_type_is_builtin(type_))),
        ],
    ))
}

pub(super) fn reflection_named_type_value(name: &str, allows_null: bool, builtin: bool) -> Value {
    Value::Object(reflection_object(
        "ReflectionNamedType",
        vec![
            ("name", reflection_string(name)),
            ("allows_null", Value::Bool(allows_null)),
            ("builtin", Value::Bool(builtin)),
        ],
    ))
}

pub(super) fn ir_type_name(type_: &IrReturnType) -> String {
    match type_ {
        IrReturnType::Int => "int".to_owned(),
        IrReturnType::Float => "float".to_owned(),
        IrReturnType::String => "string".to_owned(),
        IrReturnType::Array => "array".to_owned(),
        IrReturnType::Callable => "callable".to_owned(),
        IrReturnType::Iterable => "iterable".to_owned(),
        IrReturnType::Object => "object".to_owned(),
        IrReturnType::Bool => "bool".to_owned(),
        IrReturnType::Null => "null".to_owned(),
        IrReturnType::Void => "void".to_owned(),
        IrReturnType::Mixed => "mixed".to_owned(),
        IrReturnType::Never => "never".to_owned(),
        IrReturnType::False => "false".to_owned(),
        IrReturnType::True => "true".to_owned(),
        IrReturnType::Class { name, display_name } => {
            display_name.clone().unwrap_or_else(|| name.clone())
        }
        IrReturnType::Nullable { inner } => ir_type_name(inner),
        IrReturnType::Union { members } => members
            .iter()
            .map(ir_type_name)
            .collect::<Vec<_>>()
            .join("|"),
        IrReturnType::Intersection { members } => members
            .iter()
            .map(ir_type_name)
            .collect::<Vec<_>>()
            .join("&"),
        IrReturnType::Dnf { members } => members
            .iter()
            .map(ir_type_name)
            .collect::<Vec<_>>()
            .join("|"),
    }
}

pub(super) fn ir_type_allows_null(type_: &IrReturnType) -> bool {
    match type_ {
        IrReturnType::Null | IrReturnType::Mixed => true,
        IrReturnType::Nullable { .. } => true,
        IrReturnType::Union { members } | IrReturnType::Dnf { members } => {
            members.iter().any(ir_type_allows_null)
        }
        _ => false,
    }
}

pub(super) fn ir_type_is_builtin(type_: &IrReturnType) -> bool {
    !matches!(type_, IrReturnType::Class { .. })
}

pub(super) fn reflection_span_file(
    compiled: &CompiledUnit,
    span: php_ir::source_map::IrSpan,
) -> Value {
    compiled
        .unit()
        .files
        .get(span.file.index())
        .map(|file| reflection_string(&file.path))
        .unwrap_or(Value::Bool(false))
}

pub(super) fn reflection_span_line(
    compiled: &CompiledUnit,
    span: php_ir::source_map::IrSpan,
    end: bool,
) -> Value {
    source_span_display_line(compiled, span, end)
        .map(Value::Int)
        .unwrap_or(Value::Bool(false))
}

pub(super) fn source_span_display_line(
    compiled: &CompiledUnit,
    span: php_ir::source_map::IrSpan,
    end: bool,
) -> Option<i64> {
    compiled.source_display_line(span, end)
}

pub(super) fn runtime_source_span_display_line(
    compiled: &CompiledUnit,
    span: &RuntimeSourceSpan,
) -> Option<i64> {
    let file = span.file.as_ref()?;
    let file = compiled
        .unit()
        .files
        .iter()
        .find(|entry| entry.path == *file)?;
    compiled.source_display_line(
        php_ir::source_map::IrSpan::new(file.id, span.start, span.end),
        false,
    )
}

pub(super) fn source_span_file_line(
    compiled: &CompiledUnit,
    span: php_ir::source_map::IrSpan,
) -> Option<(String, i64)> {
    let file = compiled.unit().files.get(span.file.index())?;
    let line =
        source_span_display_line(compiled, span, false).unwrap_or_else(|| i64::from(span.start));
    Some((file.path.clone(), line))
}

pub(super) fn closure_debug_info(
    compiled: &CompiledUnit,
    function: FunctionId,
) -> Option<ClosureDebugInfo> {
    let function = compiled.unit().functions.get(function.index())?;
    if !function.flags.is_closure {
        return None;
    }
    let file = compiled.unit().files.get(function.span.file.index())?;
    let line = source_span_display_line(compiled, function.span, false)?;
    Some(ClosureDebugInfo {
        name: format!("{{closure:{}:{line}}}", file.path),
        file: file.path.clone(),
        line,
        parameters: function
            .params
            .iter()
            .map(|param| ClosureDebugParameter {
                name: param.name.clone(),
                required: param.required,
            })
            .collect(),
    })
}

pub(super) fn reflection_bool_property(object: &ObjectRef, property: &str) -> Value {
    object.get_property(property).unwrap_or(Value::Bool(false))
}

pub(super) fn reflection_short_class_name(name: &str) -> &str {
    name.rsplit_once('\\')
        .map(|(_, short)| short)
        .unwrap_or(name)
}

pub(super) fn reflection_namespace_name(name: &str) -> &str {
    name.rsplit_once('\\')
        .map(|(namespace, _)| namespace)
        .unwrap_or("")
}

pub(super) fn reflection_class_object(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
) -> Result<ObjectRef, String> {
    if php_std::ExtensionRegistry::standard_library()
        .enabled_class(class_name)
        .is_some()
    {
        return reflection_internal_class_object(class_name);
    }
    let Some(target) = lookup_class_in_state(compiled, state, class_name) else {
        return reflection_internal_class_object(class_name);
    };
    let parent = target
        .parent
        .as_ref()
        .map(|parent| {
            lookup_class_in_state(compiled, state, parent)
                .map(|entry| entry.display_name.clone())
                .unwrap_or_else(|| parent.clone())
        })
        .map(reflection_string)
        .unwrap_or(Value::Bool(false));
    Ok(reflection_object(
        "ReflectionClass",
        vec![
            (
                "name",
                Value::String(PhpString::from_test_str(&target.display_name)),
            ),
            (
                "class",
                Value::String(PhpString::from_test_str(&target.name)),
            ),
            ("parent", parent),
            (
                "attributes",
                reflection_attributes_value(compiled, &target.attributes)?,
            ),
            ("is_interface", Value::Bool(target.flags.is_interface)),
            ("is_trait", Value::Bool(false)),
            ("is_enum", Value::Bool(target.flags.is_enum)),
            ("is_abstract", Value::Bool(target.flags.is_abstract)),
            ("is_final", Value::Bool(target.flags.is_final)),
            (
                "interfaces",
                reflection_string_array(target.interfaces.iter().map(|interface| {
                    lookup_class_in_state(compiled, state, interface)
                        .map(|entry| entry.display_name.clone())
                        .unwrap_or_else(|| interface.clone())
                })),
            ),
            ("file", reflection_span_file(compiled, target.span)),
            (
                "start_line",
                reflection_span_line(compiled, target.span, false),
            ),
            (
                "end_line",
                reflection_span_line(compiled, target.span, true),
            ),
            ("is_internal", Value::Bool(false)),
            ("extension", Value::Bool(false)),
        ],
    ))
}

impl Vm {
    pub(super) fn reflection_new_object(
        &self,
        compiled: &CompiledUnit,
        class_name: &str,
        args: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<ObjectRef, String> {
        match normalize_class_name(class_name).as_str() {
            "reflectionclass" => {
                let class = reflection_string_arg(&args, 0, "ReflectionClass::__construct")?;
                if lookup_class_in_state(compiled, state, &class).is_none() {
                    self.autoload_class(compiled, &class, output, stack, state, None)
                        .map_err(|_| "E_PHP_VM_AUTOLOAD_FAILED".to_owned())?;
                }
                reflection_class_object(compiled, state, &class)
            }
            "reflectionfunction" => {
            let Some(target) = args.first() else {
                return Err(
                    "E_PHP_VM_REFLECTION_ARITY: ReflectionFunction::__construct missing argument 0"
                        .to_owned(),
                );
            };
            match target {
                Value::Callable(callable) => match callable.as_ref() {
                CallableValue::UserFunction { name } => {
                    if let Some(function_id) = compiled.lookup_function(&normalize_function_name(name))
                    {
                        let function_entry = &compiled.unit().functions[function_id.index()];
                        let object = reflection_function_object(compiled, function_entry)?;
                        object.set_property("is_closure", Value::Bool(true));
                        Ok(object)
                    } else {
                        reflection_internal_function_object(name)
                    }
                }
                CallableValue::Closure(payload) => {
                    let function = FunctionId::new(payload.function);
                    let function_entry = compiled.unit().functions.get(function.index()).ok_or_else(|| {
                        format!(
                            "E_PHP_VM_REFLECTION_UNKNOWN_FUNCTION: closure function {} is not defined",
                            function.raw()
                        )
                    })?;
                    Ok(reflection_closure_object(
                        compiled,
                        function_entry,
                        &payload.captures,
                        payload.bound_this.as_ref(),
                    )?)
                }
                CallableValue::BoundMethod { target, method, .. } => {
                    let class_name = match target {
                        CallableMethodTarget::Object(object) => object.class_name(),
                        CallableMethodTarget::Class(class_name) => class_name.clone(),
                    };
                    let owner = class_owner_in_state(compiled, state, &class_name);
                    reflection_method_object(&owner, &class_name, method)
                }
                _ => Err(
                    "E_PHP_VM_REFLECTION_UNSUPPORTED_CALLABLE: callable reflection supports user functions and closures in the reflection-clone MVP"
                        .to_owned(),
                ),
                },
                _ => {
                    let function = reflection_string_arg(&args, 0, "ReflectionFunction::__construct")?;
                    if let Some(function_id) =
                        compiled.lookup_function(&normalize_function_name(&function))
                    {
                        let function_entry = &compiled.unit().functions[function_id.index()];
                        Ok(reflection_function_object(compiled, function_entry)?)
                    } else {
                        reflection_internal_function_object(&function)
                    }
                }
            }
        }
            "reflectionmethod" => {
            let class = reflection_string_arg(&args, 0, "ReflectionMethod::__construct")?;
            let method = reflection_string_arg(&args, 1, "ReflectionMethod::__construct")?;
            let owner = class_owner_in_state(compiled, state, &class);
            reflection_method_object(&owner, &class, &method)
        }
            "reflectionproperty" => {
            let class = reflection_string_arg(&args, 0, "ReflectionProperty::__construct")?;
            let property = reflection_string_arg(&args, 1, "ReflectionProperty::__construct")?;
            reflection_property_object(compiled, &class, &property)
        }
            "reflectionclassconstant" => {
            let class = reflection_string_arg(&args, 0, "ReflectionClassConstant::__construct")?;
            let constant = reflection_string_arg(&args, 1, "ReflectionClassConstant::__construct")?;
            reflection_class_constant_object(compiled, &class, &constant)
        }
            "reflectionenum" => {
            let class = reflection_string_arg(&args, 0, "ReflectionEnum::__construct")?;
            reflection_enum_object(compiled, &class)
        }
            "reflectionenumunitcase" => {
            let class = reflection_string_arg(&args, 0, "ReflectionEnumUnitCase::__construct")?;
            let case = reflection_string_arg(&args, 1, "ReflectionEnumUnitCase::__construct")?;
            reflection_enum_case_object(compiled, &class, &case)
        }
            "reflectionenumbackedcase" => {
            let class = reflection_string_arg(&args, 0, "ReflectionEnumBackedCase::__construct")?;
            let case = reflection_string_arg(&args, 1, "ReflectionEnumBackedCase::__construct")?;
            let object = reflection_enum_case_object(compiled, &class, &case)?;
            if normalize_class_name(&object.class_name()) != "reflectionenumbackedcase" {
                return Err(format!(
                    "E_PHP_VM_REFLECTION_NOT_BACKED_ENUM_CASE: case {class}::{case} is not backed"
                ));
            }
            Ok(object)
        }
            "reflectionextension" => {
            let extension = reflection_string_arg(&args, 0, "ReflectionExtension::__construct")?;
            reflection_extension_object(&extension)
        }
            "reflectionattribute" => Err(
                "E_PHP_VM_REFLECTION_ATTRIBUTE_CONSTRUCTION: ReflectionAttribute is created by getAttributes"
                    .to_owned(),
            ),
            "reflectionparameter" => Err(
                "E_PHP_VM_REFLECTION_PARAMETER_CONSTRUCTION: ReflectionParameter direct construction is outside the method-runtime MVP"
                    .to_owned(),
            ),
            "reflectionnamedtype" => Err(
                "E_PHP_VM_REFLECTION_NAMED_TYPE_CONSTRUCTION: ReflectionNamedType is created by metadata accessors"
                    .to_owned(),
            ),
            _ => Err(format!(
                "E_PHP_VM_REFLECTION_UNKNOWN_CLASS: unsupported reflection class {class_name}"
            )),
        }
    }
}

pub(super) fn reflection_method_value(
    compiled: &CompiledUnit,
    object: &ObjectRef,
    method: &str,
    args: Vec<Value>,
    output: &mut OutputBuffer,
    stack: &CallStack,
    state: &mut ExecutionState,
    source_span: RuntimeSourceSpan,
) -> Result<Value, String> {
    let method = normalize_method_name(method);
    match normalize_class_name(&object.class_name()).as_str() {
        "reflectionattribute" => match method.as_str() {
            "getname" => Ok(object.get_property("name").unwrap_or(Value::Null)),
            "getarguments" => Ok(object
                .get_property("arguments")
                .unwrap_or_else(empty_array_value)),
            "isrepeated" => Ok(reflection_bool_property(object, "repeated")),
            "newinstance" => Err(
                "E_PHP_RUNTIME_UNSUPPORTED_ATTRIBUTE_NEWINSTANCE: attribute instantiation needs class constructor semantics"
                    .to_owned(),
            ),
            _ => Err(format!(
                "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                object.class_name(),
                method
            )),
        },
        "reflectionnamedtype" => match method.as_str() {
            "getname" | "__tostring" => Ok(object.get_property("name").unwrap_or(Value::Null)),
            "allowsnull" => Ok(reflection_bool_property(object, "allows_null")),
            "isbuiltin" => Ok(reflection_bool_property(object, "builtin")),
            _ => Err(format!(
                "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                object.class_name(),
                method
            )),
        },
        "reflectionclass" => match method.as_str() {
            "getname" => Ok(object.get_property("name").unwrap_or(Value::Null)),
            "getshortname" => {
                let name = reflection_object_string_property(object, "name")?;
                Ok(reflection_string(reflection_short_class_name(&name)))
            }
            "getnamespacename" => {
                let name = reflection_object_string_property(object, "name")?;
                Ok(reflection_string(reflection_namespace_name(&name)))
            }
            "innamespace" => {
                let name = reflection_object_string_property(object, "name")?;
                Ok(Value::Bool(!reflection_namespace_name(&name).is_empty()))
            }
            "getattributes" => Ok(object
                .get_property("attributes")
                .unwrap_or_else(empty_array_value)),
            "isinterface" => Ok(reflection_bool_property(object, "is_interface")),
            "istrait" => Ok(reflection_bool_property(object, "is_trait")),
            "isenum" => Ok(reflection_bool_property(object, "is_enum")),
            "isabstract" => Ok(reflection_bool_property(object, "is_abstract")),
            "isfinal" => Ok(reflection_bool_property(object, "is_final")),
            "isinstantiable" => Ok(Value::Bool(
                !matches!(object.get_property("is_interface"), Some(Value::Bool(true)))
                    && !matches!(object.get_property("is_abstract"), Some(Value::Bool(true))),
            )),
            "getinterfacenames" => Ok(object
                .get_property("interfaces")
                .unwrap_or_else(empty_array_value)),
            "implementsinterface" => {
                let class = reflection_object_string_property(object, "class")?;
                let interface =
                    reflection_string_arg(&args, 0, "ReflectionClass::implementsInterface")?;
                Ok(Value::Bool(class_implements_interface(
                    compiled,
                    &class,
                    &interface,
                    &mut Vec::new(),
                )?))
            }
            "getparentclass" => match object.get_property("parent") {
                Some(Value::String(parent)) => {
                    let parent = parent.to_string_lossy();
                    Ok(Value::Object(reflection_class_object(compiled, state, &parent)?))
                }
                _ => Ok(Value::Bool(false)),
            },
            "getfilename" => Ok(object.get_property("file").unwrap_or(Value::Bool(false))),
            "getstartline" => Ok(object.get_property("start_line").unwrap_or(Value::Bool(false))),
            "getendline" => Ok(object.get_property("end_line").unwrap_or(Value::Bool(false))),
            "getdoccomment" => Ok(Value::Bool(false)),
            "isinternal" => Ok(reflection_bool_property(object, "is_internal")),
            "isuserdefined" => Ok(Value::Bool(!matches!(
                object.get_property("is_internal"),
                Some(Value::Bool(true))
            ))),
            "getextensionname" => Ok(object.get_property("extension").unwrap_or(Value::Bool(false))),
            "getmethods" => {
                let class = reflection_object_string_property(object, "class")?;
                Ok(reflection_class_methods_value(compiled, state, &class)?)
            }
            "hasmethod" => {
                let class = reflection_object_string_property(object, "class")?;
                let method = reflection_string_arg(&args, 0, "ReflectionClass::hasMethod")?;
                Ok(Value::Bool(
                    lookup_method_in_state(compiled, state, &class, &method)?.is_some()
                        || internal_class_has_method(&class, &method),
                ))
            }
            "getproperties" => {
                let class = reflection_object_string_property(object, "class")?;
                Ok(reflection_class_properties_value(compiled, state, &class)?)
            }
            "getconstants" => {
                let class = reflection_object_string_property(object, "class")?;
                Ok(reflection_class_constants_value(compiled, state, &class)?)
            }
            "getconstant" => {
                let class = reflection_object_string_property(object, "class")?;
                let constant = reflection_string_arg(&args, 0, "ReflectionClass::getConstant")?;
                let constants = reflection_class_constants_value(compiled, state, &class)?;
                if let Value::Array(constants) = constants {
                    return Ok(constants
                        .get(&ArrayKey::String(PhpString::from_test_str(&constant)))
                        .cloned()
                        .unwrap_or(Value::Bool(false)));
                }
                Ok(Value::Bool(false))
            }
            "getreflectionconstants" => {
                let class = reflection_object_string_property(object, "class")?;
                Ok(reflection_class_reflection_constants_value(compiled, state, &class)?)
            }
            "getmethod" => {
                let class = reflection_object_string_property(object, "class")?;
                let method = reflection_string_arg(&args, 0, "ReflectionClass::getMethod")?;
                Ok(Value::Object(reflection_method_object(compiled, &class, &method)?))
            }
            "getproperty" => {
                let class = reflection_object_string_property(object, "class")?;
                let property = reflection_string_arg(&args, 0, "ReflectionClass::getProperty")?;
                Ok(Value::Object(reflection_property_object(
                    compiled, &class, &property,
                )?))
            }
            "getreflectionconstant" => {
                let class = reflection_object_string_property(object, "class")?;
                let constant =
                    reflection_string_arg(&args, 0, "ReflectionClass::getReflectionConstant")?;
                Ok(Value::Object(reflection_class_constant_object(
                    compiled, &class, &constant,
                )?))
            }
            _ => Err(format!(
                "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                object.class_name(),
                method
            )),
        },
        "reflectionfunction" | "reflectionmethod" => match method.as_str() {
            "getname" => Ok(object.get_property("name").unwrap_or(Value::Null)),
            "getattributes" => Ok(object
                .get_property("attributes")
                .unwrap_or_else(empty_array_value)),
            "getparameters" => reflection_function_parameters_value(object),
            "getnumberofparameters" => reflection_function_parameter_count_value(object),
            "getnumberofrequiredparameters" => {
                reflection_function_required_parameter_count_value(object)
            }
            "getreturntype" => reflection_function_return_type_value(object),
            "getfilename" => Ok(object.get_property("file").unwrap_or(Value::Bool(false))),
            "getstartline" => Ok(object.get_property("start_line").unwrap_or(Value::Bool(false))),
            "getendline" => Ok(object.get_property("end_line").unwrap_or(Value::Bool(false))),
            "getdoccomment" => Ok(Value::Bool(false)),
            "isinternal" => Ok(reflection_bool_property(object, "is_internal")),
            "isuserdefined" => Ok(Value::Bool(!matches!(
                object.get_property("is_internal"),
                Some(Value::Bool(true))
            ))),
            "getextensionname" => Ok(object.get_property("extension").unwrap_or(Value::Bool(false))),
            "ispublic" => Ok(object.get_property("is_public").unwrap_or(Value::Bool(true))),
            "isprivate" => Ok(reflection_bool_property(object, "is_private")),
            "isprotected" => Ok(reflection_bool_property(object, "is_protected")),
            "isstatic" => Ok(reflection_bool_property(object, "is_static")),
            "isabstract" => Ok(reflection_bool_property(object, "is_abstract")),
            "isfinal" => Ok(reflection_bool_property(object, "is_final")),
            "getmodifiers" => Ok(object.get_property("modifiers").unwrap_or(Value::Int(0))),
            "isclosure" => Ok(reflection_bool_property(object, "is_closure")),
            "getstaticvariables" => Ok(object
                .get_property("static_variables")
                .unwrap_or_else(empty_array_value)),
            "getclosurescopeclass" => Ok(object
                .get_property("closure_scope_class")
                .unwrap_or(Value::Bool(false))),
            "getdeclaringclass" => {
                let class = reflection_object_string_property(object, "class")?;
                Ok(Value::Object(reflection_class_object(compiled, state, &class)?))
            }
            _ => Err(format!(
                "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                object.class_name(),
                method
            )),
        },
        "reflectionproperty" | "reflectionclassconstant" | "reflectionenumunitcase"
        | "reflectionenumbackedcase" => {
            match method.as_str() {
                "getname" => Ok(object.get_property("name").unwrap_or(Value::Null)),
                "getdoccomment" => Ok(object.get_property("doc_comment").unwrap_or(Value::Bool(false))),
                "getattributes" => Ok(object
                    .get_property("attributes")
                    .unwrap_or_else(empty_array_value)),
                "getdeclaringclass" => {
                    let class = reflection_object_string_property(object, "class")?;
                    Ok(Value::Object(reflection_class_object(compiled, state, &class)?))
                }
                "gettype" => Ok(object.get_property("type").unwrap_or(Value::Null)),
                "hastype" => Ok(Value::Bool(!matches!(
                    object.get_property("type"),
                    Some(Value::Null) | None
                ))),
                "hasdefaultvalue" => Ok(reflection_bool_property(object, "has_default")),
                "getdefaultvalue" | "getvalue" => {
                    Ok(object.get_property("default").unwrap_or(Value::Null))
                }
                "getbackingvalue" => Ok(object
                    .get_property("backing_value")
                    .unwrap_or(Value::Bool(false))),
                "isenumcase" => Ok(reflection_bool_property(object, "is_enum_case")),
                "ispublic" => Ok(object.get_property("is_public").unwrap_or(Value::Bool(true))),
                "isprivate" => Ok(reflection_bool_property(object, "is_private")),
                "isprotected" => Ok(reflection_bool_property(object, "is_protected")),
                "isstatic" => Ok(reflection_bool_property(object, "is_static")),
                "isreadonly" => Ok(reflection_bool_property(object, "is_readonly")),
                "getmodifiers" => Ok(object.get_property("modifiers").unwrap_or(Value::Int(0))),
                "hashooks" => Ok(reflection_bool_property(object, "has_hooks")),
                "gethooks" => Ok(object.get_property("hooks").unwrap_or_else(empty_array_value)),
                "isvirtual" => Ok(reflection_bool_property(object, "is_virtual")),
                _ => Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                    object.class_name(),
                    method
                )),
            }
        }
        "reflectionenum" => match method.as_str() {
            "getname" => Ok(object.get_property("name").unwrap_or(Value::Null)),
            "getattributes" => Ok(object
                .get_property("attributes")
                .unwrap_or_else(empty_array_value)),
            "isbacked" => Ok(reflection_bool_property(object, "is_backed")),
            "getbackingtype" => Ok(object.get_property("backing_type").unwrap_or(Value::Null)),
            "getcases" => {
                let class = reflection_object_string_property(object, "class")?;
                Ok(reflection_enum_cases_value(compiled, &class)?)
            }
            "getcase" => {
                let class = reflection_object_string_property(object, "class")?;
                let case = reflection_string_arg(&args, 0, "ReflectionEnum::getCase")?;
                Ok(Value::Object(reflection_enum_case_object(compiled, &class, &case)?))
            }
            "hascase" => {
                let class = reflection_object_string_property(object, "class")?;
                let case = reflection_string_arg(&args, 0, "ReflectionEnum::hasCase")?;
                Ok(Value::Bool(reflection_enum_has_case(compiled, &class, &case)))
            }
            "getfilename" => Ok(object.get_property("file").unwrap_or(Value::Bool(false))),
            "getstartline" => Ok(object.get_property("start_line").unwrap_or(Value::Bool(false))),
            "getendline" => Ok(object.get_property("end_line").unwrap_or(Value::Bool(false))),
            "getdoccomment" => Ok(Value::Bool(false)),
            _ => Err(format!(
                "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                object.class_name(),
                method
            )),
        },
        "reflectionparameter" => match method.as_str() {
            "getname" => Ok(object.get_property("name").unwrap_or(Value::Null)),
            "getposition" => Ok(object.get_property("position").unwrap_or(Value::Int(0))),
            "getattributes" => Ok(object
                .get_property("attributes")
                .unwrap_or_else(empty_array_value)),
            "gettype" => Ok(object.get_property("type").unwrap_or(Value::Null)),
            "hastype" => Ok(Value::Bool(!matches!(
                object.get_property("type"),
                Some(Value::Null) | None
            ))),
            "hasdefaultvalue" | "isdefaultvalueavailable" => {
                Ok(reflection_bool_property(object, "has_default"))
            }
            "getdefaultvalue" => Ok(object.get_property("default").unwrap_or(Value::Null)),
            "isoptional" => Ok(reflection_bool_property(object, "optional")),
            "isvariadic" => Ok(reflection_bool_property(object, "variadic")),
            "ispassedbyreference" => Ok(reflection_bool_property(object, "by_ref")),
            "canbepassedbyvalue" => Ok(Value::Bool(!matches!(
                object.get_property("by_ref"),
                Some(Value::Bool(true))
            ))),
            "allowsnull" => Ok(reflection_bool_property(object, "allows_null")),
            "iscallable" => {
                emit_reflection_parameter_is_callable_deprecation(
                    compiled,
                    output,
                    stack,
                    state,
                    source_span,
                );
                Ok(Value::Bool(reflection_parameter_type_is(object, "callable")))
            }
            _ => Err(format!(
                "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                object.class_name(),
                method
            )),
        },
        "reflectionextension" => match method.as_str() {
            "getname" => Ok(object.get_property("name").unwrap_or(Value::Null)),
            "getversion" => Ok(Value::Bool(false)),
            "getconstants" => {
                let extension = reflection_object_string_property(object, "name")?;
                reflection_extension_constants_value(&extension)
            }
            "getfunctions" => {
                let extension = reflection_object_string_property(object, "name")?;
                reflection_extension_functions_value(&extension)
            }
            "getclasses" => {
                let extension = reflection_object_string_property(object, "name")?;
                reflection_extension_classes_value(&extension)
            }
            "getclassnames" => {
                let extension = reflection_object_string_property(object, "name")?;
                reflection_extension_class_names_value(&extension)
            }
            _ => Err(format!(
                "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                object.class_name(),
                method
            )),
        },
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
            object.class_name(),
            method
        )),
    }
}

#[derive(Clone)]
pub(super) struct InternalReflectionParam {
    name: &'static str,
    type_name: Option<&'static str>,
    allows_null: bool,
    optional: bool,
    default: Value,
    by_ref: bool,
    variadic: bool,
}

#[derive(Clone)]
pub(super) struct InternalReflectionSignature {
    return_type: Option<&'static str>,
    return_allows_null: bool,
    params: Vec<InternalReflectionParam>,
}

pub(super) fn internal_function_signature(name: &str) -> InternalReflectionSignature {
    if let Some(metadata) = php_std::arginfo::function_metadata_indexed(name) {
        return signature_from_params(metadata.return_type, metadata.params);
    }
    InternalReflectionSignature {
        return_type: Some("mixed"),
        return_allows_null: true,
        params: Vec::new(),
    }
}

pub(super) fn emit_reflection_parameter_is_callable_deprecation(
    compiled: &CompiledUnit,
    output: &mut OutputBuffer,
    stack: &CallStack,
    state: &mut ExecutionState,
    source_span: RuntimeSourceSpan,
) {
    if !error_reporting_allows(state, php_runtime::api::PHP_E_DEPRECATED) {
        return;
    }
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_VM_REFLECTION_PARAMETER_IS_CALLABLE_DEPRECATED",
        RuntimeSeverity::Deprecation,
        "Method ReflectionParameter::isCallable() is deprecated since 8.0, use ReflectionParameter::getType() instead",
        source_span,
        stack_trace(compiled, stack),
        None,
    );
    emit_vm_diagnostic(
        output,
        state,
        &diagnostic,
        php_runtime::api::PhpDiagnosticChannel::Deprecated,
        php_runtime::api::PHP_E_DEPRECATED,
    );
    state.diagnostics.push(diagnostic);
}

pub(super) fn reflection_parameter_type_is(object: &ObjectRef, expected: &str) -> bool {
    let Some(Value::Object(type_object)) = object.get_property("type") else {
        return false;
    };
    let Some(Value::String(name)) = type_object.get_property("name") else {
        return false;
    };
    name.to_string_lossy().eq_ignore_ascii_case(expected)
}

pub(super) fn internal_method_signature(
    class_name: &str,
    method_name: &str,
) -> InternalReflectionSignature {
    if let Some(metadata) = php_std::generated::arginfo::method_metadata(class_name, method_name) {
        return signature_from_params(metadata.return_type, metadata.params);
    }
    if normalize_class_name(class_name) == "ffi" && normalize_method_name(method_name) == "cdef" {
        return InternalReflectionSignature {
            return_type: Some("FFI"),
            return_allows_null: false,
            params: vec![
                InternalReflectionParam {
                    name: "code",
                    type_name: Some("string"),
                    allows_null: false,
                    optional: true,
                    default: Value::String(PhpString::from_test_str("")),
                    by_ref: false,
                    variadic: false,
                },
                InternalReflectionParam {
                    name: "lib",
                    type_name: Some("string"),
                    allows_null: true,
                    optional: true,
                    default: Value::Null,
                    by_ref: false,
                    variadic: false,
                },
            ],
        };
    }
    InternalReflectionSignature {
        return_type: Some("mixed"),
        return_allows_null: true,
        params: Vec::new(),
    }
}

pub(super) fn signature_from_params(
    return_type: &'static str,
    params: &'static [php_std::generated::arginfo::GeneratedParamMetadata],
) -> InternalReflectionSignature {
    InternalReflectionSignature {
        return_type: reflection_type_name(return_type),
        return_allows_null: reflection_type_allows_null(return_type),
        params: params
            .iter()
            .map(|param| InternalReflectionParam {
                name: param.name,
                type_name: reflection_type_name(param.type_decl),
                allows_null: reflection_type_allows_null(param.type_decl),
                optional: param.optional || param.variadic,
                default: reflection_default_value(param.default_value),
                by_ref: param.by_ref,
                variadic: param.variadic,
            })
            .collect(),
    }
}

pub(super) fn reflection_type_name(type_decl: &'static str) -> Option<&'static str> {
    let type_decl = type_decl.trim();
    if type_decl.is_empty() {
        return None;
    }
    Some(type_decl.strip_prefix('?').unwrap_or(type_decl))
}

pub(super) fn reflection_type_allows_null(type_decl: &str) -> bool {
    let lower = type_decl.to_ascii_lowercase();
    lower == "mixed" || lower == "null" || lower.starts_with('?') || lower.contains("|null")
}

pub(super) fn reflection_default_value(default: Option<&str>) -> Value {
    let Some(default) = default else {
        return Value::Null;
    };
    match default {
        "null" | "NULL" => Value::Null,
        "false" | "FALSE" => Value::Bool(false),
        "true" | "TRUE" => Value::Bool(true),
        value if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 => {
            Value::String(PhpString::from(&value[1..value.len() - 1]))
        }
        value if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 => {
            Value::String(PhpString::from(&value[1..value.len() - 1]))
        }
        value => value.parse::<i64>().map_or(Value::Null, Value::Int),
    }
}

pub(super) fn reflection_internal_parameters_value(params: &[InternalReflectionParam]) -> Value {
    let mut array = PhpArray::new();
    for (position, param) in params.iter().enumerate() {
        let has_default = param.optional && !param.variadic;
        array.append(Value::Object(reflection_object(
            "ReflectionParameter",
            vec![
                ("name", reflection_string(param.name)),
                ("position", Value::Int(position as i64)),
                ("attributes", empty_array_value()),
                (
                    "type",
                    param.type_name.map_or(Value::Null, |type_name| {
                        reflection_named_type_value(type_name, param.allows_null, true)
                    }),
                ),
                ("has_default", Value::Bool(has_default)),
                (
                    "default",
                    if has_default {
                        param.default.clone()
                    } else {
                        Value::Null
                    },
                ),
                ("optional", Value::Bool(param.optional)),
                ("by_ref", Value::Bool(param.by_ref)),
                ("variadic", Value::Bool(param.variadic)),
                (
                    "allows_null",
                    Value::Bool(param.type_name.is_none_or(|_| param.allows_null)),
                ),
            ],
        )));
    }
    Value::Array(array)
}

pub(super) fn reflection_internal_function_object(function: &str) -> Result<ObjectRef, String> {
    let registry = php_std::ExtensionRegistry::standard_library();
    let descriptor = registry.enabled_php_function(function).ok_or_else(|| {
        format!("E_PHP_VM_REFLECTION_UNKNOWN_FUNCTION: function {function} is not defined")
    })?;
    let signature = internal_function_signature(descriptor.name());
    Ok(reflection_object(
        "ReflectionFunction",
        vec![
            ("name", reflection_string(descriptor.name())),
            ("attributes", empty_array_value()),
            (
                "parameters",
                reflection_internal_parameters_value(&signature.params),
            ),
            ("parameter_count", Value::Int(signature.params.len() as i64)),
            (
                "required_parameter_count",
                Value::Int(
                    signature
                        .params
                        .iter()
                        .filter(|param| !param.optional)
                        .count() as i64,
                ),
            ),
            (
                "return_type",
                signature.return_type.map_or(Value::Null, |name| {
                    reflection_named_type_value(name, signature.return_allows_null, true)
                }),
            ),
            ("file", Value::Bool(false)),
            ("start_line", Value::Bool(false)),
            ("end_line", Value::Bool(false)),
            ("is_closure", Value::Bool(false)),
            ("static_variables", empty_array_value()),
            ("closure_scope_class", Value::Null),
            ("is_internal", Value::Bool(true)),
            ("extension", reflection_string(descriptor.extension())),
        ],
    ))
}

pub(super) fn reflection_internal_function_listing_object(
    function: &str,
) -> Result<ObjectRef, String> {
    let registry = php_std::ExtensionRegistry::standard_library();
    let descriptor = registry.enabled_php_function(function).ok_or_else(|| {
        format!("E_PHP_VM_REFLECTION_UNKNOWN_FUNCTION: function {function} is not defined")
    })?;
    Ok(reflection_object(
        "ReflectionFunction",
        vec![
            ("name", reflection_string(descriptor.name())),
            ("attributes", empty_array_value()),
            ("file", Value::Bool(false)),
            ("start_line", Value::Bool(false)),
            ("end_line", Value::Bool(false)),
            ("is_closure", Value::Bool(false)),
            ("static_variables", empty_array_value()),
            ("closure_scope_class", Value::Null),
            ("is_internal", Value::Bool(true)),
            ("extension", reflection_string(descriptor.extension())),
        ],
    ))
}

pub(super) fn reflection_lazy_internal_function_signature(
    object: &ObjectRef,
) -> Result<Option<InternalReflectionSignature>, String> {
    if !matches!(object.get_property("is_internal"), Some(Value::Bool(true)))
        || object.get_property("class").is_some()
    {
        return Ok(None);
    }
    let name = reflection_object_string_property(object, "name")?;
    Ok(Some(internal_function_signature(&name)))
}

pub(super) fn reflection_function_parameters_value(object: &ObjectRef) -> Result<Value, String> {
    if let Some(parameters) = object.get_property("parameters") {
        return Ok(parameters);
    }
    if let Some(signature) = reflection_lazy_internal_function_signature(object)? {
        return Ok(reflection_internal_parameters_value(&signature.params));
    }
    Ok(empty_array_value())
}

pub(super) fn reflection_function_parameter_count_value(
    object: &ObjectRef,
) -> Result<Value, String> {
    if let Some(parameter_count) = object.get_property("parameter_count") {
        return Ok(parameter_count);
    }
    if let Some(signature) = reflection_lazy_internal_function_signature(object)? {
        return Ok(Value::Int(signature.params.len() as i64));
    }
    Ok(Value::Int(0))
}

pub(super) fn reflection_function_required_parameter_count_value(
    object: &ObjectRef,
) -> Result<Value, String> {
    if let Some(required_parameter_count) = object.get_property("required_parameter_count") {
        return Ok(required_parameter_count);
    }
    if let Some(signature) = reflection_lazy_internal_function_signature(object)? {
        return Ok(Value::Int(
            signature
                .params
                .iter()
                .filter(|param| !param.optional)
                .count() as i64,
        ));
    }
    Ok(Value::Int(0))
}

pub(super) fn reflection_function_return_type_value(object: &ObjectRef) -> Result<Value, String> {
    if let Some(return_type) = object.get_property("return_type") {
        return Ok(return_type);
    }
    if let Some(signature) = reflection_lazy_internal_function_signature(object)? {
        return Ok(signature.return_type.map_or(Value::Null, |name| {
            reflection_named_type_value(name, signature.return_allows_null, true)
        }));
    }
    Ok(Value::Null)
}

pub(super) fn reflection_internal_class_object(class_name: &str) -> Result<ObjectRef, String> {
    let registry = php_std::ExtensionRegistry::standard_library();
    let descriptor = registry.enabled_class(class_name).ok_or_else(|| {
        format!("E_PHP_VM_REFLECTION_UNKNOWN_CLASS: class {class_name} is not defined")
    })?;
    let is_interface = descriptor.kind() == php_std::ClassKind::Interface;
    let is_trait = descriptor.kind() == php_std::ClassKind::Trait;
    let is_enum = descriptor.kind() == php_std::ClassKind::Enum;
    Ok(reflection_object(
        "ReflectionClass",
        vec![
            ("name", reflection_string(descriptor.name())),
            ("class", reflection_string(descriptor.name())),
            (
                "parent",
                internal_throwable_parent(descriptor.name())
                    .map(reflection_string)
                    .unwrap_or(Value::Bool(false)),
            ),
            ("attributes", empty_array_value()),
            ("is_interface", Value::Bool(is_interface)),
            ("is_trait", Value::Bool(is_trait)),
            ("is_enum", Value::Bool(is_enum)),
            ("is_abstract", Value::Bool(is_interface || is_trait)),
            ("is_final", Value::Bool(false)),
            (
                "interfaces",
                reflection_string_array(internal_class_interfaces(descriptor.name())),
            ),
            ("file", Value::Bool(false)),
            ("start_line", Value::Bool(false)),
            ("end_line", Value::Bool(false)),
            ("is_internal", Value::Bool(true)),
            ("extension", reflection_string(descriptor.extension())),
        ],
    ))
}

pub(super) fn internal_class_interfaces(class_name: &str) -> Vec<String> {
    match normalize_class_name(class_name).as_str() {
        "iterator" | "iteratoraggregate" => {
            ["Traversable"].into_iter().map(str::to_owned).collect()
        }
        "seekableiterator" | "recursiveiterator" => {
            ["Iterator"].into_iter().map(str::to_owned).collect()
        }
        "arrayobject" => [
            "IteratorAggregate",
            "ArrayAccess",
            "Serializable",
            "Countable",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        "arrayiterator" => [
            "SeekableIterator",
            "ArrayAccess",
            "Serializable",
            "Countable",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        "recursivearrayiterator" => [
            "RecursiveIterator",
            "SeekableIterator",
            "ArrayAccess",
            "Serializable",
            "Countable",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        "splfileobject" | "spltempfileobject" => ["RecursiveIterator", "SeekableIterator"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        "ziparchive" => ["Countable"].into_iter().map(str::to_owned).collect(),
        _ => Vec::new(),
    }
}

pub(super) fn internal_class_methods(class_name: &str) -> Vec<InternalClassMethod> {
    let mut methods: Vec<_> = php_std::generated::arginfo::class_methods(class_name)
        .into_iter()
        .map(|method| InternalClassMethod {
            name: method.name,
            is_static: method.is_static,
        })
        .collect();
    if normalize_class_name(class_name) == "ffi" {
        methods.extend(FFI_STATIC_METHODS.iter().map(|name| InternalClassMethod {
            name,
            is_static: true,
        }));
    }
    if normalize_class_name(class_name) == "redis" {
        methods.extend(
            REDIS_INSTANCE_METHODS
                .iter()
                .map(|name| InternalClassMethod {
                    name,
                    is_static: false,
                }),
        );
    }
    if normalize_class_name(class_name) == "memcached" {
        methods.extend(
            MEMCACHED_INSTANCE_METHODS
                .iter()
                .map(|name| InternalClassMethod {
                    name,
                    is_static: false,
                }),
        );
    }
    if normalize_class_name(class_name) == "imagick" {
        methods.extend(
            IMAGICK_INSTANCE_METHODS
                .iter()
                .map(|name| InternalClassMethod {
                    name,
                    is_static: false,
                }),
        );
    }
    methods
}

#[derive(Clone, Copy)]
pub(super) struct InternalClassMethod {
    name: &'static str,
    is_static: bool,
}

pub(super) const FFI_STATIC_METHODS: &[&str] = &[
    "cdef",
    "load",
    "new",
    "cast",
    "typeof",
    "addr",
    "sizeof",
    "alignof",
    "memcpy",
    "memcmp",
    "memset",
    "string",
    "isNull",
    "arrayType",
    "free",
    "scope",
    "type",
];

pub(super) const REDIS_INSTANCE_METHODS: &[&str] = &[
    "__construct",
    "connect",
    "pconnect",
    "auth",
    "select",
    "close",
    "ping",
    "getMode",
    "isConnected",
    "set",
    "setex",
    "setnx",
    "get",
    "mget",
    "getMultiple",
    "mset",
    "del",
    "delete",
    "unlink",
    "exists",
    "expire",
    "pexpire",
    "persist",
    "ttl",
    "pttl",
    "incr",
    "incrBy",
    "decr",
    "decrBy",
    "hSet",
    "hGet",
    "hGetAll",
    "hDel",
    "hExists",
    "lPush",
    "rPush",
    "lPop",
    "rPop",
    "lLen",
    "sAdd",
    "sMembers",
    "sIsMember",
    "sContains",
    "sRem",
    "sRemove",
    "zAdd",
    "zRange",
    "flushDB",
    "flushAll",
    "multi",
    "pipeline",
    "exec",
    "discard",
    "scan",
    "setOption",
    "getOption",
];

pub(super) const MEMCACHED_INSTANCE_METHODS: &[&str] = &[
    "__construct",
    "addServer",
    "addServers",
    "getServerList",
    "set",
    "add",
    "replace",
    "get",
    "getMulti",
    "setMulti",
    "delete",
    "deleteMulti",
    "increment",
    "decrement",
    "touch",
    "flush",
    "setOption",
    "getOption",
    "setOptions",
    "getResultCode",
    "getResultMessage",
    "append",
    "prepend",
    "cas",
    "getStats",
    "getVersion",
];

pub(super) const IMAGICK_INSTANCE_METHODS: &[&str] = &[
    "__construct",
    "readImage",
    "readImageBlob",
    "writeImage",
    "getImagesBlob",
    "resizeImage",
    "cropImage",
    "thumbnailImage",
    "identifyImage",
    "getImageWidth",
    "getImageHeight",
    "getImageFormat",
    "setImageFormat",
    "stripImage",
    "clear",
    "destroy",
];

pub(super) fn internal_class_has_method(class_name: &str, method_name: &str) -> bool {
    let normalized = normalize_method_name(method_name);
    internal_class_methods(class_name)
        .into_iter()
        .any(|method| normalize_method_name(method.name) == normalized)
}

pub(super) fn reflection_internal_method_object(
    class_name: &str,
    method_name: &str,
) -> Result<ObjectRef, String> {
    let registry = php_std::ExtensionRegistry::standard_library();
    let descriptor = registry.enabled_class(class_name).ok_or_else(|| {
        format!("E_PHP_VM_REFLECTION_UNKNOWN_CLASS: class {class_name} is not defined")
    })?;
    let method = internal_class_methods(descriptor.name())
        .into_iter()
        .find(|method| normalize_method_name(method.name) == normalize_method_name(method_name))
        .ok_or_else(|| {
            format!(
                "E_PHP_VM_REFLECTION_UNKNOWN_METHOD: method {}::{} is not defined",
                descriptor.name(),
                method_name
            )
        })?;
    let signature = internal_method_signature(descriptor.name(), method.name);
    Ok(reflection_object(
        "ReflectionMethod",
        vec![
            ("class", reflection_string(descriptor.name())),
            ("name", reflection_string(method.name)),
            ("attributes", empty_array_value()),
            (
                "parameters",
                reflection_internal_parameters_value(&signature.params),
            ),
            ("parameter_count", Value::Int(signature.params.len() as i64)),
            (
                "required_parameter_count",
                Value::Int(
                    signature
                        .params
                        .iter()
                        .filter(|param| !param.optional)
                        .count() as i64,
                ),
            ),
            (
                "return_type",
                signature.return_type.map_or(Value::Null, |name| {
                    reflection_named_type_value(name, signature.return_allows_null, true)
                }),
            ),
            ("file", Value::Bool(false)),
            ("start_line", Value::Bool(false)),
            ("end_line", Value::Bool(false)),
            ("is_public", Value::Bool(true)),
            ("is_private", Value::Bool(false)),
            ("is_protected", Value::Bool(false)),
            ("is_static", Value::Bool(method.is_static)),
            ("is_abstract", Value::Bool(false)),
            ("is_final", Value::Bool(false)),
            (
                "modifiers",
                reflection_method_modifiers(false, false, method.is_static, false, false),
            ),
            ("is_internal", Value::Bool(true)),
            ("extension", reflection_string(descriptor.extension())),
        ],
    ))
}

pub(super) fn reflection_extension_object(extension: &str) -> Result<ObjectRef, String> {
    let registry = php_std::ExtensionRegistry::standard_library();
    let descriptor = registry
        .extension_case_insensitive(extension)
        .ok_or_else(|| {
            format!("E_PHP_VM_REFLECTION_UNKNOWN_EXTENSION: extension {extension} is not defined")
        })?;
    if !registry.is_extension_enabled(descriptor.name()) {
        return Err(format!(
            "E_PHP_VM_REFLECTION_UNKNOWN_EXTENSION: extension {extension} is not loaded"
        ));
    }
    Ok(reflection_object(
        "ReflectionExtension",
        vec![(
            "name",
            reflection_string(reflection_extension_display_name(descriptor.name())),
        )],
    ))
}

pub(super) fn reflection_extension_display_name(extension: &str) -> &str {
    if extension.eq_ignore_ascii_case("reflection") {
        "Reflection"
    } else {
        extension
    }
}

pub(super) fn reflection_extension_functions_value(extension: &str) -> Result<Value, String> {
    let registry = php_std::ExtensionRegistry::standard_library();
    let descriptor = registry
        .extension_case_insensitive(extension)
        .ok_or_else(|| {
            format!("E_PHP_VM_REFLECTION_UNKNOWN_EXTENSION: extension {extension} is not defined")
        })?;
    let mut array = PhpArray::new();
    for function in descriptor.functions() {
        if function.visibility() != php_std::SymbolVisibility::PhpVisible {
            continue;
        }
        reflection_assoc_insert(
            &mut array,
            function.name(),
            Value::Object(reflection_internal_function_listing_object(
                function.name(),
            )?),
        );
    }
    Ok(Value::Array(array))
}

pub(super) fn reflection_extension_constants_value(extension: &str) -> Result<Value, String> {
    let registry = php_std::ExtensionRegistry::standard_library();
    let descriptor = registry
        .extension_case_insensitive(extension)
        .ok_or_else(|| {
            format!("E_PHP_VM_REFLECTION_UNKNOWN_EXTENSION: extension {extension} is not defined")
        })?;
    let mut array = PhpArray::new();
    for constant in descriptor.constants() {
        let Some(value) = constant.value() else {
            continue;
        };
        reflection_assoc_insert(
            &mut array,
            constant.name(),
            php_std::constants::constant_to_value(value),
        );
    }
    Ok(Value::Array(array))
}

pub(super) fn reflection_extension_classes_value(extension: &str) -> Result<Value, String> {
    let registry = php_std::ExtensionRegistry::standard_library();
    let descriptor = registry
        .extension_case_insensitive(extension)
        .ok_or_else(|| {
            format!("E_PHP_VM_REFLECTION_UNKNOWN_EXTENSION: extension {extension} is not defined")
        })?;
    let mut array = PhpArray::new();
    for class in descriptor.classes() {
        reflection_assoc_insert(
            &mut array,
            class.name(),
            Value::Object(reflection_internal_class_object(class.name())?),
        );
    }
    Ok(Value::Array(array))
}

pub(super) fn reflection_extension_class_names_value(extension: &str) -> Result<Value, String> {
    let registry = php_std::ExtensionRegistry::standard_library();
    let descriptor = registry
        .extension_case_insensitive(extension)
        .ok_or_else(|| {
            format!("E_PHP_VM_REFLECTION_UNKNOWN_EXTENSION: extension {extension} is not defined")
        })?;
    Ok(reflection_string_array(
        descriptor
            .classes()
            .iter()
            .map(|class| class.name().to_owned()),
    ))
}

pub(super) fn reflection_class_methods_value(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
) -> Result<Value, String> {
    let Some(class) = lookup_class_in_state(compiled, state, class_name) else {
        let methods = internal_class_methods(class_name)
            .into_iter()
            .map(|method| reflection_internal_method_object(class_name, method.name))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(reflection_objects_array(methods));
    };
    let methods = class
        .methods
        .iter()
        .map(|method| reflection_method_object(compiled, &class.name, &method.name))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(reflection_objects_array(methods))
}

pub(super) fn reflection_class_properties_value(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
) -> Result<Value, String> {
    let Some(class) = lookup_class_in_state(compiled, state, class_name) else {
        php_std::ExtensionRegistry::standard_library()
            .enabled_class(class_name)
            .ok_or_else(|| {
                format!("E_PHP_VM_REFLECTION_UNKNOWN_CLASS: class {class_name} is not defined")
            })?;
        return Ok(empty_array_value());
    };
    let properties = class
        .properties
        .iter()
        .map(|property| reflection_property_object(compiled, &class.name, &property.name))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(reflection_objects_array(properties))
}

pub(super) fn reflection_class_constants_value(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
) -> Result<Value, String> {
    let Some(class) = lookup_class_in_state(compiled, state, class_name) else {
        php_std::ExtensionRegistry::standard_library()
            .enabled_class(class_name)
            .ok_or_else(|| {
                format!("E_PHP_VM_REFLECTION_UNKNOWN_CLASS: class {class_name} is not defined")
            })?;
        return Ok(empty_array_value());
    };
    let mut array = PhpArray::new();
    for resolved in reflection_class_constants_in_hierarchy(compiled, state, class)? {
        let constant = resolved.constant;
        let value = constant
            .value
            .map(|constant| {
                let owner = class_owner_in_state(compiled, state, &resolved.class.name);
                constant_value(owner.unit(), constant)
            })
            .transpose()?
            .unwrap_or(Value::Null);
        reflection_assoc_insert(&mut array, &constant.name, value);
    }
    Ok(Value::Array(array))
}

pub(super) fn reflection_class_reflection_constants_value(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
) -> Result<Value, String> {
    let Some(class) = lookup_class_in_state(compiled, state, class_name) else {
        php_std::ExtensionRegistry::standard_library()
            .enabled_class(class_name)
            .ok_or_else(|| {
                format!("E_PHP_VM_REFLECTION_UNKNOWN_CLASS: class {class_name} is not defined")
            })?;
        return Ok(empty_array_value());
    };
    let constants = reflection_class_constants_in_hierarchy(compiled, state, class)?
        .into_iter()
        .map(|constant| {
            reflection_class_constant_object(
                compiled,
                &constant.class.name,
                &constant.constant.name,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(reflection_objects_array(constants))
}

pub(super) fn reflection_class_constants_in_hierarchy(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: CompiledClass,
) -> Result<Vec<ResolvedConstantOwned>, String> {
    let mut constants = Vec::new();
    let mut seen_classes = Vec::new();
    let mut seen_names = BTreeSet::new();
    collect_reflection_class_constants(
        compiled,
        state,
        class,
        &mut seen_classes,
        &mut seen_names,
        &mut constants,
    )?;
    Ok(constants)
}

pub(super) fn collect_reflection_class_constants(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: CompiledClass,
    seen_classes: &mut Vec<String>,
    seen_names: &mut BTreeSet<String>,
    constants: &mut Vec<ResolvedConstantOwned>,
) -> Result<(), String> {
    let class_name = normalize_class_name(&class.name);
    if seen_classes.iter().any(|name| name == &class_name) {
        return Err(format!(
            "E_PHP_VM_CLASS_INHERITANCE_CYCLE: class {} participates in an inheritance cycle",
            class.name
        ));
    }
    seen_classes.push(class_name);
    for constant in &class.constants {
        if seen_names.insert(constant.name.clone()) {
            constants.push(ResolvedConstantOwned {
                class: (*class).clone(),
                constant: constant.clone(),
            });
        }
    }
    if let Some(parent_name) = class.parent.as_deref() {
        let Some(parent) = lookup_class_in_state(compiled, state, parent_name) else {
            return Err(format!(
                "E_PHP_VM_UNKNOWN_PARENT_CLASS: class {} extends missing class {}",
                class.name, parent_name
            ));
        };
        collect_reflection_class_constants(
            compiled,
            state,
            parent,
            seen_classes,
            seen_names,
            constants,
        )?;
    }
    seen_classes.pop();
    Ok(())
}

pub(super) fn reflection_enum_cases_value(
    compiled: &CompiledUnit,
    class_name: &str,
) -> Result<Value, String> {
    let class = compiled.lookup_class(class_name).ok_or_else(|| {
        format!("E_PHP_VM_REFLECTION_UNKNOWN_CLASS: class {class_name} is not defined")
    })?;
    let cases = class
        .enum_cases
        .iter()
        .map(|case| reflection_enum_case_object(compiled, &class.name, &case.name))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(reflection_objects_array(cases))
}

pub(super) fn reflection_enum_has_case(
    compiled: &CompiledUnit,
    class_name: &str,
    case_name: &str,
) -> bool {
    compiled.lookup_class(class_name).is_some_and(|class| {
        class
            .enum_cases
            .iter()
            .any(|case| case.name.eq_ignore_ascii_case(case_name))
    })
}

pub(super) fn reflection_visibility_modifiers(
    is_private: bool,
    is_protected: bool,
    is_static: bool,
    is_readonly: bool,
) -> Value {
    let mut modifiers = 0;
    if is_private {
        modifiers |= 4;
    } else if is_protected {
        modifiers |= 2;
    } else {
        modifiers |= 1;
    }
    if is_static {
        modifiers |= 16;
    }
    if is_readonly {
        modifiers |= 128;
    }
    Value::Int(modifiers)
}

pub(super) fn reflection_method_modifiers(
    is_private: bool,
    is_protected: bool,
    is_static: bool,
    is_abstract: bool,
    is_final: bool,
) -> Value {
    let mut modifiers = 0;
    if is_private {
        modifiers |= 4;
    } else if is_protected {
        modifiers |= 2;
    } else {
        modifiers |= 1;
    }
    if is_static {
        modifiers |= 16;
    }
    if is_final {
        modifiers |= 32;
    }
    if is_abstract {
        modifiers |= 64;
    }
    Value::Int(modifiers)
}

pub(super) fn reflection_property_hooks_value(hooks: &php_ir::module::ClassPropertyHooks) -> Value {
    let mut array = PhpArray::new();
    if hooks.get.is_some() {
        reflection_assoc_insert(&mut array, "get", reflection_string("get"));
    }
    if hooks.set.is_some() {
        reflection_assoc_insert(&mut array, "set", reflection_string("set"));
    }
    Value::Array(array)
}

pub(super) fn reflection_enum_backing_type_value(
    backing_type: Option<php_ir::module::ClassEnumBackingType>,
) -> Value {
    let Some(backing_type) = backing_type else {
        return Value::Null;
    };
    let name = match backing_type {
        php_ir::module::ClassEnumBackingType::Int => "int",
        php_ir::module::ClassEnumBackingType::String => "string",
    };
    Value::Object(reflection_object(
        "ReflectionNamedType",
        vec![
            ("name", reflection_string(name)),
            ("allows_null", Value::Bool(false)),
            ("builtin", Value::Bool(true)),
        ],
    ))
}

pub(super) fn reflection_static_variables_value(captures: &[ClosureCaptureValue]) -> Value {
    let mut array = PhpArray::new();
    for capture in captures {
        let value = capture
            .value()
            .cloned()
            .or_else(|| capture.reference().map(|reference| reference.get()))
            .unwrap_or(Value::Null);
        reflection_assoc_insert(&mut array, &capture.name, value);
    }
    Value::Array(array)
}

pub(super) fn reflection_function_object(
    compiled: &CompiledUnit,
    function: &IrFunction,
) -> Result<ObjectRef, String> {
    let parameters = reflection_parameters_value(compiled, &function.params)?;
    let parameter_count = function.params.len() as i64;
    let required_parameter_count = function
        .params
        .iter()
        .filter(|param| param.required)
        .count() as i64;
    Ok(reflection_object(
        "ReflectionFunction",
        vec![
            (
                "name",
                Value::String(PhpString::from_test_str(&function.name)),
            ),
            (
                "attributes",
                reflection_attributes_value(compiled, &function.attributes)?,
            ),
            ("parameters", parameters),
            ("parameter_count", Value::Int(parameter_count)),
            (
                "required_parameter_count",
                Value::Int(required_parameter_count),
            ),
            (
                "return_type",
                reflection_type_value(function.return_type.as_ref()),
            ),
            ("file", reflection_span_file(compiled, function.span)),
            (
                "start_line",
                reflection_span_line(compiled, function.span, false),
            ),
            (
                "end_line",
                reflection_span_line(compiled, function.span, true),
            ),
            ("is_closure", Value::Bool(function.flags.is_closure)),
            ("static_variables", empty_array_value()),
            ("closure_scope_class", Value::Null),
            ("is_internal", Value::Bool(false)),
            ("extension", Value::Bool(false)),
        ],
    ))
}

pub(super) fn reflection_closure_object(
    compiled: &CompiledUnit,
    function: &IrFunction,
    captures: &[ClosureCaptureValue],
    bound_this: Option<&ObjectRef>,
) -> Result<ObjectRef, String> {
    let object = reflection_function_object(compiled, function)?;
    object.set_property("name", reflection_string("{closure}"));
    object.set_property("is_closure", Value::Bool(true));
    object.set_property(
        "static_variables",
        reflection_static_variables_value(captures),
    );
    let scope_class = bound_this
        .filter(|object| is_std_class_object(object))
        .map(|_| reflection_internal_class_object("Closure").map(Value::Object))
        .transpose()?
        .unwrap_or(Value::Null);
    object.set_property("closure_scope_class", scope_class);
    Ok(object)
}

pub(super) fn reflection_method_object(
    compiled: &CompiledUnit,
    class_name: &str,
    method_name: &str,
) -> Result<ObjectRef, String> {
    let Some(class) = compiled.lookup_class(class_name) else {
        return reflection_internal_method_object(class_name, method_name);
    };
    let method = class
        .methods
        .iter()
        .find(|method| method.name == normalize_method_name(method_name))
        .ok_or_else(|| {
            format!(
                "E_PHP_VM_REFLECTION_UNKNOWN_METHOD: method {}::{} is not defined",
                class.name, method_name
            )
        })?;
    let function = &compiled.unit().functions[method.function.index()];
    let parameters = reflection_parameters_value(compiled, &function.params)?;
    let parameter_count = function.params.len() as i64;
    let required_parameter_count = function
        .params
        .iter()
        .filter(|param| param.required)
        .count() as i64;
    Ok(reflection_object(
        "ReflectionMethod",
        vec![
            (
                "class",
                Value::String(PhpString::from_test_str(&class.display_name)),
            ),
            ("name", Value::String(PhpString::from_test_str(method_name))),
            (
                "attributes",
                reflection_attributes_value(compiled, &method.attributes)?,
            ),
            ("parameters", parameters),
            ("parameter_count", Value::Int(parameter_count)),
            (
                "required_parameter_count",
                Value::Int(required_parameter_count),
            ),
            (
                "return_type",
                reflection_type_value(function.return_type.as_ref()),
            ),
            ("file", reflection_span_file(compiled, function.span)),
            (
                "start_line",
                reflection_span_line(compiled, function.span, false),
            ),
            (
                "end_line",
                reflection_span_line(compiled, function.span, true),
            ),
            (
                "is_public",
                Value::Bool(!method.flags.is_private && !method.flags.is_protected),
            ),
            ("is_private", Value::Bool(method.flags.is_private)),
            ("is_protected", Value::Bool(method.flags.is_protected)),
            ("is_static", Value::Bool(method.flags.is_static)),
            ("is_abstract", Value::Bool(method.flags.is_abstract)),
            ("is_final", Value::Bool(method.flags.is_final)),
            (
                "modifiers",
                reflection_method_modifiers(
                    method.flags.is_private,
                    method.flags.is_protected,
                    method.flags.is_static,
                    method.flags.is_abstract,
                    method.flags.is_final,
                ),
            ),
            ("is_internal", Value::Bool(false)),
            ("extension", Value::Bool(false)),
        ],
    ))
}

pub(super) fn reflection_property_object(
    compiled: &CompiledUnit,
    class_name: &str,
    property_name: &str,
) -> Result<ObjectRef, String> {
    let class = compiled.lookup_class(class_name).ok_or_else(|| {
        format!("E_PHP_VM_REFLECTION_UNKNOWN_CLASS: class {class_name} is not defined")
    })?;
    let property = class
        .properties
        .iter()
        .find(|property| property.name == property_name)
        .ok_or_else(|| {
            format!(
                "E_PHP_VM_REFLECTION_UNKNOWN_PROPERTY: property {}::{} is not defined",
                class.name, property_name
            )
        })?;
    let default = property
        .default
        .map(|constant| constant_value(compiled.unit(), constant))
        .transpose()?;
    Ok(reflection_object(
        "ReflectionProperty",
        vec![
            (
                "class",
                Value::String(PhpString::from_test_str(&class.name)),
            ),
            (
                "name",
                Value::String(PhpString::from_test_str(property_name)),
            ),
            (
                "attributes",
                reflection_attributes_value(compiled, &property.attributes)?,
            ),
            ("type", reflection_type_value(property.type_.as_ref())),
            ("has_type", Value::Bool(property.type_.is_some())),
            ("has_default", Value::Bool(default.is_some())),
            ("default", default.unwrap_or(Value::Null)),
            (
                "is_public",
                Value::Bool(!property.flags.is_private && !property.flags.is_protected),
            ),
            ("is_private", Value::Bool(property.flags.is_private)),
            ("is_protected", Value::Bool(property.flags.is_protected)),
            ("is_static", Value::Bool(property.flags.is_static)),
            ("is_readonly", Value::Bool(property.flags.is_readonly)),
            (
                "modifiers",
                reflection_visibility_modifiers(
                    property.flags.is_private,
                    property.flags.is_protected,
                    property.flags.is_static,
                    property.flags.is_readonly,
                ),
            ),
            (
                "has_hooks",
                Value::Bool(property.hooks.get.is_some() || property.hooks.set.is_some()),
            ),
            ("hooks", reflection_property_hooks_value(&property.hooks)),
            ("is_virtual", Value::Bool(!property.hooks.backed)),
        ],
    ))
}

pub(super) fn reflection_class_constant_object(
    compiled: &CompiledUnit,
    class_name: &str,
    constant_name: &str,
) -> Result<ObjectRef, String> {
    let class = compiled.lookup_class(class_name).ok_or_else(|| {
        format!("E_PHP_VM_REFLECTION_UNKNOWN_CLASS: class {class_name} is not defined")
    })?;
    let Some(constant) = class
        .constants
        .iter()
        .find(|constant| constant.name == constant_name)
    else {
        if class
            .enum_cases
            .iter()
            .any(|case| case.name.eq_ignore_ascii_case(constant_name))
        {
            return reflection_enum_case_object(compiled, &class.name, constant_name);
        }
        return Err(format!(
            "E_PHP_VM_REFLECTION_UNKNOWN_CONSTANT: constant {}::{} is not defined",
            class.name, constant_name
        ));
    };
    let value = constant
        .value
        .map(|constant| constant_value(compiled.unit(), constant))
        .transpose()?;
    Ok(reflection_object(
        "ReflectionClassConstant",
        vec![
            (
                "class",
                Value::String(PhpString::from_test_str(&class.name)),
            ),
            (
                "name",
                Value::String(PhpString::from_test_str(constant_name)),
            ),
            (
                "attributes",
                reflection_attributes_value(compiled, &constant.attributes)?,
            ),
            (
                "doc_comment",
                constant
                    .doc_comment
                    .as_ref()
                    .map_or(Value::Bool(false), reflection_string),
            ),
            ("has_default", Value::Bool(value.is_some())),
            ("default", value.unwrap_or(Value::Null)),
            (
                "is_public",
                Value::Bool(!constant.flags.is_private && !constant.flags.is_protected),
            ),
            ("is_private", Value::Bool(constant.flags.is_private)),
            ("is_protected", Value::Bool(constant.flags.is_protected)),
            ("is_static", Value::Bool(true)),
            ("is_enum_case", Value::Bool(false)),
            (
                "modifiers",
                reflection_visibility_modifiers(
                    constant.flags.is_private,
                    constant.flags.is_protected,
                    false,
                    false,
                ),
            ),
        ],
    ))
}

pub(super) fn reflection_enum_object(
    compiled: &CompiledUnit,
    class_name: &str,
) -> Result<ObjectRef, String> {
    let target = compiled.lookup_class(class_name).ok_or_else(|| {
        format!("E_PHP_VM_REFLECTION_UNKNOWN_CLASS: class {class_name} is not defined")
    })?;
    if !target.flags.is_enum {
        return Err(format!(
            "E_PHP_VM_REFLECTION_NOT_ENUM: class {class_name} is not an enum"
        ));
    }
    Ok(reflection_object(
        "ReflectionEnum",
        vec![
            (
                "name",
                Value::String(PhpString::from_test_str(&target.display_name)),
            ),
            (
                "class",
                Value::String(PhpString::from_test_str(&target.name)),
            ),
            (
                "attributes",
                reflection_attributes_value(compiled, &target.attributes)?,
            ),
            ("is_backed", Value::Bool(target.enum_backing_type.is_some())),
            (
                "backing_type",
                reflection_enum_backing_type_value(target.enum_backing_type),
            ),
            ("file", reflection_span_file(compiled, target.span)),
            (
                "start_line",
                reflection_span_line(compiled, target.span, false),
            ),
            (
                "end_line",
                reflection_span_line(compiled, target.span, true),
            ),
        ],
    ))
}

pub(super) fn reflection_enum_case_object(
    compiled: &CompiledUnit,
    class_name: &str,
    case_name: &str,
) -> Result<ObjectRef, String> {
    let class = compiled.lookup_class(class_name).ok_or_else(|| {
        format!("E_PHP_VM_REFLECTION_UNKNOWN_CLASS: class {class_name} is not defined")
    })?;
    let case = class
        .enum_cases
        .iter()
        .find(|case| case.name == case_name)
        .ok_or_else(|| {
            format!(
                "E_PHP_VM_REFLECTION_UNKNOWN_ENUM_CASE: case {}::{} is not defined",
                class.name, case_name
            )
        })?;
    let backing_value = case
        .value
        .map(|constant| constant_value(compiled.unit(), constant))
        .transpose()?;
    let reflection_class = if backing_value.is_some() {
        "ReflectionEnumBackedCase"
    } else {
        "ReflectionEnumUnitCase"
    };
    Ok(reflection_object(
        reflection_class,
        vec![
            (
                "class",
                Value::String(PhpString::from_test_str(&class.name)),
            ),
            ("name", Value::String(PhpString::from_test_str(case_name))),
            (
                "attributes",
                reflection_attributes_value(compiled, &case.attributes)?,
            ),
            ("backing_value", backing_value.unwrap_or(Value::Bool(false))),
            ("has_default", Value::Bool(false)),
            ("default", Value::Null),
            ("is_enum_case", Value::Bool(true)),
            ("is_public", Value::Bool(true)),
            ("is_private", Value::Bool(false)),
            ("is_protected", Value::Bool(false)),
            ("is_static", Value::Bool(true)),
            ("is_readonly", Value::Bool(true)),
            (
                "modifiers",
                reflection_visibility_modifiers(false, false, true, true),
            ),
        ],
    ))
}

pub(super) fn reflection_parameters_value(
    compiled: &CompiledUnit,
    params: &[IrParam],
) -> Result<Value, String> {
    let mut array = PhpArray::new();
    for (position, param) in params.iter().enumerate() {
        let default = param.default.as_ref().map(inline_constant_value);
        array.append(Value::Object(reflection_object(
            "ReflectionParameter",
            vec![
                ("name", Value::String(PhpString::from_test_str(&param.name))),
                ("position", Value::Int(position as i64)),
                (
                    "attributes",
                    reflection_attributes_value(compiled, &param.attributes)?,
                ),
                ("type", reflection_type_value(param.type_.as_ref())),
                ("has_default", Value::Bool(default.is_some())),
                ("default", default.unwrap_or(Value::Null)),
                ("optional", Value::Bool(!param.required)),
                ("by_ref", Value::Bool(param.by_ref)),
                ("variadic", Value::Bool(param.variadic)),
                (
                    "allows_null",
                    Value::Bool(param.type_.as_ref().is_none_or(ir_type_allows_null)),
                ),
            ],
        )));
    }
    Ok(Value::Array(array))
}

pub(super) fn reflection_attributes_value(
    compiled: &CompiledUnit,
    attributes: &[php_ir::module::AttributeEntry],
) -> Result<Value, String> {
    let mut array = PhpArray::new();
    for attribute in
        runtime_attributes(attributes, &|value| constant_value(compiled.unit(), value))?
    {
        array.append(Value::Object(reflection_attribute_object(attribute)));
    }
    Ok(Value::Array(array))
}

pub(super) fn reflection_attribute_object(attribute: RuntimeAttributeEntry) -> ObjectRef {
    let mut arguments = PhpArray::new();
    for argument in attribute.arguments {
        arguments.append(argument);
    }
    let name = attribute.name;
    reflection_object(
        "ReflectionAttribute",
        vec![
            ("name", Value::String(PhpString::from_test_str(&name))),
            ("arguments", Value::Array(arguments)),
            ("repeated", Value::Bool(attribute.repeated_on_target)),
        ],
    )
}

pub(super) fn reflection_attribute_constructor_args(
    object: &ObjectRef,
) -> Result<Vec<CallArgument>, String> {
    let Some(arguments) = object.get_property("arguments") else {
        return Ok(Vec::new());
    };
    let Value::Array(arguments) = effective_value(&arguments) else {
        return Err(format!(
            "E_PHP_VM_REFLECTION_METADATA_TYPE: {} arguments metadata is not an array",
            object.class_name()
        ));
    };
    Ok(arguments
        .iter()
        .map(|(_, value)| CallArgument::positional(effective_value(value)))
        .collect())
}

pub(super) fn reflection_new_instance_args(
    values: Vec<Value>,
) -> Result<Vec<CallArgument>, String> {
    let Some(arguments) = values.first() else {
        return Ok(Vec::new());
    };
    let Value::Array(arguments) = effective_value(arguments) else {
        return Err(format!(
            "E_PHP_VM_TYPE_ERROR: ReflectionClass::newInstanceArgs(): Argument #1 ($args) must be of type array, {} given",
            value_type_name(arguments)
        ));
    };
    Ok(arguments
        .iter()
        .map(|(_, value)| CallArgument::positional(effective_value(value)))
        .collect())
}

pub(super) fn reflection_object_string_property(
    object: &ObjectRef,
    property: &str,
) -> Result<String, String> {
    let Some(value) = object.get_property(property) else {
        return Err(format!(
            "E_PHP_VM_REFLECTION_METADATA_MISSING: {} missing property {property}",
            object.class_name()
        ));
    };
    Ok(to_string(&value)?.to_string_lossy())
}

pub(super) fn reflection_object_to_string(object: &ObjectRef) -> Result<String, String> {
    match normalize_class_name(&object.class_name()).as_str() {
        "reflectionfunction" | "reflectionmethod" => reflection_function_to_string(object),
        "reflectionnamedtype" => reflection_object_string_property(object, "name"),
        _ => Err(format!(
            "E_PHP_VM_REFLECTION_TOSTRING_UNSUPPORTED: object of class {} cannot be converted to string",
            object.class_name()
        )),
    }
}

pub(super) fn reflection_function_to_string(object: &ObjectRef) -> Result<String, String> {
    let name = reflection_object_string_property(object, "name")?;
    let is_closure = matches!(object.get_property("is_closure"), Some(Value::Bool(true)));
    let header_name = &name;
    let function_kind = if is_closure { "Closure" } else { "Function" };
    let file = reflection_to_string_scalar(object.get_property("file").as_ref())
        .unwrap_or_else(|| "<unknown>".to_owned());
    let start_line = reflection_to_string_scalar(object.get_property("start_line").as_ref())
        .unwrap_or_else(|| "0".to_owned());
    let end_line = reflection_to_string_scalar(object.get_property("end_line").as_ref())
        .unwrap_or_else(|| start_line.clone());
    let params = match object.get_property("parameters") {
        Some(Value::Array(params)) => params,
        _ => PhpArray::new(),
    };
    let mut out = String::new();
    out.push_str(&format!(
        "{function_kind} [ <user> function {header_name} ] {{\n"
    ));
    out.push_str(&format!("  @@ {file} {start_line} - {end_line}\n\n"));
    out.push_str(&format!("  - Parameters [{}] {{\n", params.len()));
    for (index, (_, value)) in params.iter().enumerate() {
        let Value::Object(param) = value else {
            continue;
        };
        out.push_str("    ");
        out.push_str(&reflection_parameter_to_string(index, param)?);
        out.push('\n');
    }
    out.push_str("  }\n}");
    Ok(out)
}

pub(super) fn reflection_parameter_to_string(
    index: usize,
    param: &ObjectRef,
) -> Result<String, String> {
    let name = reflection_object_string_property(param, "name")?;
    let optional = matches!(param.get_property("optional"), Some(Value::Bool(true)));
    let by_ref = matches!(param.get_property("by_ref"), Some(Value::Bool(true)));
    let variadic = matches!(param.get_property("variadic"), Some(Value::Bool(true)));
    let required_label = if optional { "<optional>" } else { "<required>" };
    let type_prefix = match param.get_property("type") {
        Some(Value::Object(type_object)) => {
            let type_name = reflection_object_string_property(&type_object, "name")?;
            format!("{type_name} ")
        }
        _ => String::new(),
    };
    let by_ref_prefix = if by_ref { "&" } else { "" };
    let variadic_prefix = if variadic { "..." } else { "" };
    Ok(format!(
        "Parameter #{index} [ {required_label} {type_prefix}{by_ref_prefix}{variadic_prefix}${name} ]"
    ))
}

pub(super) fn reflection_to_string_scalar(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(value)) => Some(value.to_string_lossy()),
        Some(Value::Int(value)) => Some(value.to_string()),
        Some(Value::Bool(false)) | Some(Value::Null) | None => None,
        Some(Value::Bool(true)) => Some("1".to_owned()),
        _ => None,
    }
}
