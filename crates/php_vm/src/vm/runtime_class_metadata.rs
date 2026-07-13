use super::prelude::*;

pub(super) fn is_fiber_runtime_class(class_name: &str) -> bool {
    normalize_class_name(class_name) == "fiber"
}

pub(super) fn is_closure_runtime_class(class_name: &str) -> bool {
    normalize_class_name(class_name) == "closure"
}

/// Returns true when a statically named `new` expression can lower to the
/// dense `NewObject` opcode. Builtin runtime classes keep their dedicated
/// rich-interpreter construction paths; everything else resolves through
/// the shared userland instantiation helpers at execution time (including
/// autoload, abstract/interface/enum guards, and constructor dispatch).
pub(crate) fn dense_new_object_lowering_supported(class_name: &str) -> bool {
    !(is_special_static_class_name(class_name)
        || is_closure_runtime_class(class_name)
        || is_fiber_runtime_class(class_name)
        || is_reflection_runtime_class(class_name)
        || is_phar_runtime_class(class_name)
        || is_zip_runtime_class(class_name)
        || is_xml_runtime_class(class_name)
        || is_pdo_runtime_class(class_name)
        || is_sqlite_runtime_class(class_name)
        || is_spl_iterator_runtime_class(class_name)
        || is_spl_container_runtime_class(class_name)
        || is_spl_heap_runtime_class(class_name)
        || is_spl_file_runtime_class(class_name)
        || is_std_class_runtime_class(class_name)
        || is_php_token_runtime_class(class_name)
        || is_fileinfo_runtime_class(class_name)
        || is_imagick_runtime_class(class_name)
        || is_xsl_runtime_class(class_name)
        || is_soap_runtime_class(class_name)
        || is_date_time_runtime_class(class_name))
}

pub(super) fn runtime_class_entry(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
    constant_value: &impl Fn(ConstId) -> Result<Value, String>,
    _class_constant_reference_value: &impl Fn(&ClassConstantReference) -> Result<Value, String>,
    _named_constant_reference_value: &impl Fn(&NamedConstantReference) -> Result<Value, String>,
) -> Result<RuntimeClassEntry, RuntimeClassEntryError> {
    let mut lineage = Vec::new();
    collect_class_lineage(compiled, state, class, &mut lineage)
        .map_err(RuntimeClassEntryError::new)?;
    let mut properties = Vec::new();
    let mut constants = Vec::new();
    for lineage_class in &lineage {
        let owner = class_owner_in_state(compiled, state, &lineage_class.name);
        push_runtime_properties(&owner, state, lineage_class, &mut properties)?;
        push_runtime_constants(&owner, state, lineage_class, &mut constants)?;
    }
    Ok(RuntimeClassEntry {
        name: class.name.clone().into(),
        parent: class.parent.clone(),
        interfaces: class.interfaces.clone(),
        methods: class
            .methods
            .iter()
            .map(|method| {
                Ok(RuntimeClassMethodEntry {
                    name: method.name.clone(),
                    origin_class: method.origin_class.clone(),
                    function_id: method.function.raw(),
                    flags: RuntimeClassMethodFlags {
                        is_static: method.flags.is_static,
                        is_private: method.flags.is_private,
                        is_protected: method.flags.is_protected,
                        is_abstract: method.flags.is_abstract,
                        is_final: method.flags.is_final,
                    },
                    attributes: runtime_attributes(&method.attributes, constant_value)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        properties,
        constants,
        enum_cases: push_runtime_enum_cases(class, constant_value)?,
        attributes: runtime_attributes(&class.attributes, constant_value)?,
        enum_backing_type: class.enum_backing_type.map(|backing| match backing {
            php_ir::module::ClassEnumBackingType::Int => RuntimeClassEnumBackingType::Int,
            php_ir::module::ClassEnumBackingType::String => RuntimeClassEnumBackingType::String,
        }),
        constructor_id: class.constructor.map(|function| function.raw()),
        flags: RuntimeClassFlags {
            is_abstract: class.flags.is_abstract || class.flags.is_trait,
            is_final: class.flags.is_final,
            is_readonly: class.flags.is_readonly,
            is_interface: class.flags.is_interface,
            is_enum: class.flags.is_enum,
        },
    })
}

pub(super) fn collect_class_lineage(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
    lineage: &mut Vec<php_ir::module::ClassEntry>,
) -> Result<(), String> {
    collect_class_lineage_inner(compiled, state, class, lineage, &mut Vec::new())
}

pub(super) fn collect_class_lineage_inner(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
    lineage: &mut Vec<php_ir::module::ClassEntry>,
    seen: &mut Vec<String>,
) -> Result<(), String> {
    let normalized = normalize_class_name(&class.name);
    if seen.iter().any(|name| name == &normalized) {
        return Err(format!(
            "E_PHP_VM_CLASS_INHERITANCE_CYCLE: class {} participates in an inheritance cycle",
            class.name
        ));
    }
    seen.push(normalized);
    if let Some(parent_name) = class.parent.as_deref() {
        let Some(parent) = lookup_class_in_state(compiled, state, parent_name) else {
            return Err(format!(
                "E_PHP_VM_UNKNOWN_PARENT_CLASS: class {} extends missing class {}",
                class.name, parent_name
            ));
        };
        collect_class_lineage_inner(compiled, state, &parent, lineage, seen)?;
    }
    lineage.push(class.clone());
    seen.pop();
    Ok(())
}

pub(super) fn collect_class_lineage_compiled<'a>(
    compiled: &'a CompiledUnit,
    class: &'a php_ir::module::ClassEntry,
    lineage: &mut Vec<&'a php_ir::module::ClassEntry>,
) -> Result<(), String> {
    collect_class_lineage_compiled_inner(compiled, class, lineage, &mut Vec::new())
}

pub(super) fn collect_class_lineage_compiled_inner<'a>(
    compiled: &'a CompiledUnit,
    class: &'a php_ir::module::ClassEntry,
    lineage: &mut Vec<&'a php_ir::module::ClassEntry>,
    seen: &mut Vec<String>,
) -> Result<(), String> {
    let normalized = normalize_class_name(&class.name);
    if seen.iter().any(|name| name == &normalized) {
        return Err(format!(
            "E_PHP_VM_CLASS_INHERITANCE_CYCLE: class {} participates in an inheritance cycle",
            class.name
        ));
    }
    seen.push(normalized);
    if let Some(parent_name) = class.parent.as_deref() {
        let Some(parent) = compiled.lookup_class(parent_name) else {
            if internal_runtime_class_entry(&normalize_class_name(parent_name)).is_some() {
                lineage.push(class);
                seen.pop();
                return Ok(());
            }
            return Err(format!(
                "E_PHP_VM_UNKNOWN_PARENT_CLASS: class {} extends missing class {}",
                class.name, parent_name
            ));
        };
        collect_class_lineage_compiled_inner(compiled, parent, lineage, seen)?;
    }
    lineage.push(class);
    seen.pop();
    Ok(())
}

pub(super) fn push_runtime_properties(
    owner: &CompiledUnit,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
    properties: &mut Vec<RuntimeClassPropertyEntry>,
) -> Result<(), RuntimeClassEntryError> {
    for property in &class.properties {
        if (property.hooks.get.is_some() || property.hooks.set.is_some())
            && !property.hooks.backed
            && !property.flags.is_static
        {
            properties.push(RuntimeClassPropertyEntry {
                name: property.name.clone(),
                default: Value::Uninitialized,
                type_: ir_runtime_type(property.type_.as_ref()),
                flags: RuntimeClassPropertyFlags {
                    is_static: property.flags.is_static,
                    is_private: property.flags.is_private,
                    is_protected: property.flags.is_protected,
                    set_is_private: property.flags.set_is_private,
                    set_is_protected: property.flags.set_is_protected,
                    is_readonly: property.flags.is_readonly,
                    is_typed: property.flags.is_typed,
                },
                hooks: RuntimeClassPropertyHooks {
                    get_function_id: property.hooks.get.map(|id| id.index() as u32),
                    set_function_id: property.hooks.set.map(|id| id.index() as u32),
                    backed: false,
                },
                attributes: runtime_attributes(&property.attributes, &|value| {
                    constant_value(owner.unit(), value)
                })
                .map_err(RuntimeClassEntryError::new)?,
            });
            continue;
        }
        let default = if let Some(default) = property.default {
            constant_value(owner.unit(), default).map_err(RuntimeClassEntryError::new)?
        } else if let Some(reference) = &property.default_class_constant {
            class_constant_reference_value(owner, state, reference)
                .map_err(RuntimeClassEntryError::new)?
        } else if let Some(reference) = &property.default_named_constant {
            named_constant_reference_value(owner, state, reference)
                .map_err(RuntimeClassEntryError::new)?
        } else if let Some(expr) = &property.default_expr {
            deferred_const_expr_value(owner, state, expr).map_err(RuntimeClassEntryError::new)?
        } else if property.flags.is_typed {
            Value::Uninitialized
        } else {
            Value::Null
        };
        properties.push(RuntimeClassPropertyEntry {
            name: property_storage_name(class, property),
            default,
            type_: ir_runtime_type(property.type_.as_ref()),
            flags: RuntimeClassPropertyFlags {
                is_static: property.flags.is_static,
                is_private: property.flags.is_private,
                is_protected: property.flags.is_protected,
                set_is_private: property.flags.set_is_private,
                set_is_protected: property.flags.set_is_protected,
                is_readonly: property.flags.is_readonly,
                is_typed: property.flags.is_typed,
            },
            hooks: RuntimeClassPropertyHooks {
                get_function_id: property.hooks.get.map(|id| id.index() as u32),
                set_function_id: property.hooks.set.map(|id| id.index() as u32),
                backed: property.hooks.backed,
            },
            attributes: runtime_attributes(&property.attributes, &|value| {
                constant_value(owner.unit(), value)
            })
            .map_err(RuntimeClassEntryError::new)?,
        });
    }
    Ok(())
}

pub(super) fn push_runtime_constants(
    owner: &CompiledUnit,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
    constants: &mut Vec<RuntimeClassConstantEntry>,
) -> Result<(), RuntimeClassEntryError> {
    for constant in &class.constants {
        let value = if let Some(value) = constant.value {
            constant_value(owner.unit(), value).map_err(|message| {
                RuntimeClassEntryError::with_constant_initializer_span(message, constant.span)
            })?
        } else if let Some(reference) = &constant.value_class_constant {
            class_constant_reference_value(owner, state, reference).map_err(|message| {
                RuntimeClassEntryError::with_constant_initializer_span(message, constant.span)
            })?
        } else if let Some(reference) = &constant.value_named_constant {
            named_constant_reference_value(owner, state, reference).map_err(|message| {
                RuntimeClassEntryError::with_constant_initializer_span(message, constant.span)
            })?
        } else {
            Value::Null
        };
        constants.push(RuntimeClassConstantEntry {
            name: constant.name.clone(),
            value,
            flags: RuntimeClassConstantFlags {
                is_private: constant.flags.is_private,
                is_protected: constant.flags.is_protected,
            },
            attributes: runtime_attributes(&constant.attributes, &|value| {
                constant_value(owner.unit(), value)
            })
            .map_err(RuntimeClassEntryError::new)?,
        });
    }
    Ok(())
}

pub(super) fn push_runtime_enum_cases(
    class: &php_ir::module::ClassEntry,
    constant_value: &impl Fn(ConstId) -> Result<Value, String>,
) -> Result<Vec<RuntimeClassEnumCaseEntry>, String> {
    class
        .enum_cases
        .iter()
        .map(|case| {
            Ok(RuntimeClassEnumCaseEntry {
                name: case.name.clone(),
                value: case.value.map(constant_value).transpose()?,
                attributes: runtime_attributes(&case.attributes, constant_value)?,
            })
        })
        .collect()
}

pub(super) fn runtime_attributes(
    attributes: &[php_ir::module::AttributeEntry],
    constant_value: &impl Fn(ConstId) -> Result<Value, String>,
) -> Result<Vec<RuntimeAttributeEntry>, String> {
    attributes
        .iter()
        .map(|attribute| {
            let arguments = attribute
                .arguments
                .iter()
                .map(|argument| constant_value(*argument))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(RuntimeAttributeEntry {
                name: attribute.name.clone(),
                resolved_name: attribute.resolved_name.clone(),
                fallback_name: attribute.fallback_name.clone(),
                arguments,
                repeated_on_target: attribute.repeated_on_target,
                span: Some((
                    attribute.span.file.raw(),
                    attribute.span.start,
                    attribute.span.end,
                )),
            })
        })
        .collect()
}

pub(super) fn internal_runtime_class_entry(normalized: &str) -> Option<php_ir::module::ClassEntry> {
    if is_std_class_runtime_class(normalized) {
        return Some(empty_internal_class_entry("stdClass", None, None, false));
    }
    if is_hash_context_runtime_class(normalized) {
        return Some(internal_hash_context_class_entry());
    }
    if is_php_token_runtime_class(normalized) {
        return Some(internal_php_token_class_entry());
    }
    if is_redis_runtime_class(normalized) {
        return Some(empty_internal_class_entry("Redis", None, None, false));
    }
    if is_memcached_runtime_class(normalized) {
        return Some(empty_internal_class_entry("Memcached", None, None, false));
    }
    if is_soap_runtime_class(normalized) {
        let display_name = soap_display_name(&normalize_class_name(normalized));
        return Some(empty_internal_class_entry(display_name, None, None, false));
    }
    if is_supported_spl_runtime_class(normalized) {
        return Some(internal_spl_class_entry(normalized));
    }
    if normalize_class_name(normalized) == "datetimeinterface"
        || is_date_time_runtime_class(normalized)
    {
        return Some(internal_date_time_class_entry(normalized));
    }
    if normalize_class_name(normalized) == "throwable"
        || internal_throwable_instanceof(normalized, "throwable").is_some()
    {
        return Some(internal_throwable_class_entry(normalized));
    }
    None
}

pub(super) fn internal_runtime_parent_is_known(
    _class: &php_ir::module::ClassEntry,
    parent_name: &str,
) -> bool {
    internal_runtime_class_entry(&normalize_class_name(parent_name)).is_some()
}

pub(super) fn internal_spl_class_entry(normalized: &str) -> php_ir::module::ClassEntry {
    if is_spl_interface_runtime_class(normalized) {
        let display_name = display_class_name(normalized);
        let mut entry = empty_internal_class_entry(&display_name, None, None, true);
        entry.interfaces = internal_class_interfaces(&display_name)
            .into_iter()
            .map(|interface| normalize_class_name(&interface))
            .collect();
        return entry;
    }
    let runtime_class = if is_spl_iterator_runtime_class(normalized) {
        spl_iterator_class(normalized)
    } else if is_spl_container_runtime_class(normalized) {
        spl_container_class(normalized)
    } else if is_spl_heap_runtime_class(normalized) {
        spl_heap_class(normalized)
    } else {
        spl_file_class(normalized)
    };
    let display_name = if is_spl_iterator_runtime_class(normalized) {
        spl_iterator_display_name(normalized)
    } else if is_spl_container_runtime_class(normalized) {
        spl_container_display_name(normalized)
    } else if is_spl_heap_runtime_class(normalized) {
        spl_heap_display_name(normalized)
    } else {
        spl_file_display_name(normalized)
    };
    let parent_display_name = runtime_class.parent.as_deref().map(display_class_name);
    let mut entry = empty_internal_class_entry(
        display_name,
        runtime_class.parent.clone(),
        parent_display_name,
        false,
    );
    entry.interfaces = runtime_class.interfaces.clone();
    entry
}

pub(super) fn internal_throwable_class_entry(normalized: &str) -> php_ir::module::ClassEntry {
    let display_name = internal_throwable_display_name(normalized);
    let parent = internal_throwable_parent(normalized).map(normalize_class_name);
    let parent_display_name = internal_throwable_parent(normalized).map(display_class_name);
    empty_internal_class_entry(
        &display_name,
        parent,
        parent_display_name,
        normalize_class_name(normalized) == "throwable",
    )
}

pub(super) fn empty_internal_class_entry(
    display_name: &str,
    parent: Option<String>,
    parent_display_name: Option<String>,
    is_interface: bool,
) -> php_ir::module::ClassEntry {
    php_ir::module::ClassEntry {
        id: ClassId::new(u32::MAX),
        name: normalize_class_name(display_name),
        display_name: display_name.to_owned(),
        parent,
        parent_display_name,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor: None,
        flags: php_ir::module::ClassFlags {
            is_interface,
            ..php_ir::module::ClassFlags::default()
        },
        span: IrSpan::default(),
    }
}

pub(super) fn internal_hash_context_class_entry() -> php_ir::module::ClassEntry {
    let mut entry = empty_internal_class_entry("HashContext", None, None, false);
    let constructor = FunctionId::new(u32::MAX);
    entry.methods.push(php_ir::module::ClassMethodEntry {
        name: "__construct".to_owned(),
        origin_class: entry.name.clone(),
        function: constructor,
        flags: php_ir::module::ClassMethodFlags {
            is_private: true,
            ..php_ir::module::ClassMethodFlags::default()
        },
        attributes: Vec::new(),
    });
    entry.constructor = Some(constructor);
    entry
}

pub(super) fn internal_php_token_class_entry() -> php_ir::module::ClassEntry {
    let mut entry = empty_internal_class_entry("PhpToken", None, None, false);
    entry.interfaces.push(normalize_class_name("Stringable"));
    let constructor = FunctionId::new(u32::MAX);
    entry.methods.push(php_ir::module::ClassMethodEntry {
        name: "__construct".to_owned(),
        origin_class: entry.name.clone(),
        function: constructor,
        flags: php_ir::module::ClassMethodFlags {
            is_final: true,
            ..php_ir::module::ClassMethodFlags::default()
        },
        attributes: Vec::new(),
    });
    entry.constructor = Some(constructor);
    entry
}

pub(super) fn internal_enum_class_entry(normalized: &str) -> Option<php_ir::module::ClassEntry> {
    match normalized {
        "roundingmode" => Some(rounding_mode_class_entry()),
        "pdo" => Some(internal_empty_class_entry("pdo", "PDO")),
        "pdostatement" => Some(internal_empty_class_entry("pdostatement", "PDOStatement")),
        "pdorow" => Some(internal_empty_class_entry("pdorow", "PDORow")),
        "mysqli" => Some(internal_empty_class_entry("mysqli", "mysqli")),
        "mysqli_driver" => Some(internal_empty_class_entry("mysqli_driver", "mysqli_driver")),
        "mysqli_result" => Some(internal_empty_class_entry("mysqli_result", "mysqli_result")),
        "mysqli_stmt" => Some(internal_empty_class_entry("mysqli_stmt", "mysqli_stmt")),
        "mysqli_warning" => Some(internal_empty_class_entry(
            "mysqli_warning",
            "mysqli_warning",
        )),
        "redis" => Some(internal_empty_class_entry("redis", "Redis")),
        "redisexception" => Some(internal_empty_class_entry(
            "redisexception",
            "RedisException",
        )),
        "memcached" => Some(internal_empty_class_entry("memcached", "Memcached")),
        "memcachedexception" => Some(internal_empty_class_entry(
            "memcachedexception",
            "MemcachedException",
        )),
        "finfo" => Some(internal_empty_class_entry("finfo", "finfo")),
        "messagepack" => Some(internal_empty_class_entry("messagepack", "MessagePack")),
        "messagepackunpacker" => Some(internal_empty_class_entry(
            "messagepackunpacker",
            "MessagePackUnpacker",
        )),
        "imagick" => Some(internal_empty_class_entry("imagick", "Imagick")),
        "imagickdraw" => Some(internal_empty_class_entry("imagickdraw", "ImagickDraw")),
        "imagickpixel" => Some(internal_empty_class_entry("imagickpixel", "ImagickPixel")),
        "imagickpixeliterator" => Some(internal_empty_class_entry(
            "imagickpixeliterator",
            "ImagickPixelIterator",
        )),
        "imagickexception" => Some(internal_empty_class_entry(
            "imagickexception",
            "ImagickException",
        )),
        "phar" => {
            let mut entry = internal_empty_class_entry("phar", "Phar");
            entry.interfaces.push(normalize_class_name("ArrayAccess"));
            entry.interfaces.push(normalize_class_name("Countable"));
            Some(entry)
        }
        "phardata" => Some(internal_empty_class_entry("phardata", "PharData")),
        "pharfileinfo" => {
            let mut entry = internal_empty_class_entry("pharfileinfo", "PharFileInfo");
            entry.parent = Some(normalize_class_name("SplFileInfo"));
            entry.parent_display_name = Some("SplFileInfo".to_owned());
            Some(entry)
        }
        "ziparchive" => Some(internal_empty_class_entry("ziparchive", "ZipArchive")),
        "gdimage" => Some(internal_empty_class_entry("gdimage", "GdImage")),
        "domdocument" => Some(internal_empty_class_entry("domdocument", "DOMDocument")),
        "domnode" => Some(internal_empty_class_entry("domnode", "DOMNode")),
        "domattr" => Some(internal_empty_class_entry("domattr", "DOMAttr")),
        "domtext" => Some(internal_empty_class_entry("domtext", "DOMText")),
        "domcomment" => Some(internal_empty_class_entry("domcomment", "DOMComment")),
        "domcdatasection" => Some(internal_empty_class_entry(
            "domcdatasection",
            "DOMCdataSection",
        )),
        "domelement" => Some(internal_empty_class_entry("domelement", "DOMElement")),
        "domnodelist" => Some(internal_empty_class_entry("domnodelist", "DOMNodeList")),
        "domnamednodemap" => Some(internal_empty_class_entry(
            "domnamednodemap",
            "DOMNamedNodeMap",
        )),
        "domxpath" => Some(internal_empty_class_entry("domxpath", "DOMXPath")),
        "simplexmlelement" => Some(internal_empty_class_entry(
            "simplexmlelement",
            "SimpleXMLElement",
        )),
        "xmlparser" => Some(internal_empty_class_entry("xmlparser", "XMLParser")),
        "xmlreader" => Some(internal_empty_class_entry("xmlreader", "XMLReader")),
        "xmlwriter" => Some(internal_empty_class_entry("xmlwriter", "XMLWriter")),
        "xsltprocessor" => Some(internal_empty_class_entry("xsltprocessor", "XSLTProcessor")),
        "normalizer" => Some(internal_empty_class_entry("normalizer", "Normalizer")),
        "ffi" => Some(internal_empty_class_entry("ffi", "FFI")),
        "ffi\\cdata" => Some(internal_empty_class_entry("ffi\\cdata", "FFI\\CData")),
        "ffi\\ctype" => Some(internal_empty_class_entry("ffi\\ctype", "FFI\\CType")),
        "ffi\\exception" => Some(internal_empty_class_entry(
            "ffi\\exception",
            "FFI\\Exception",
        )),
        "ffi\\parserexception" => Some(internal_empty_class_entry(
            "ffi\\parserexception",
            "FFI\\ParserException",
        )),
        "shmop" => Some(internal_empty_class_entry("shmop", "Shmop")),
        "sysvmessagequeue" => Some(internal_empty_class_entry(
            "sysvmessagequeue",
            "SysvMessageQueue",
        )),
        "sysvsemaphore" => Some(internal_empty_class_entry("sysvsemaphore", "SysvSemaphore")),
        "sysvsharedmemory" => Some(internal_empty_class_entry(
            "sysvsharedmemory",
            "SysvSharedMemory",
        )),
        _ => None,
    }
}

pub(super) fn internal_empty_class_entry(
    name: &str,
    display_name: &str,
) -> php_ir::module::ClassEntry {
    php_ir::module::ClassEntry {
        id: ClassId::new(u32::MAX - 1),
        name: name.to_owned(),
        display_name: display_name.to_owned(),
        parent: None,
        parent_display_name: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor: None,
        flags: php_ir::module::ClassFlags::default(),
        span: IrSpan::default(),
    }
}

pub(super) fn rounding_mode_class_entry() -> php_ir::module::ClassEntry {
    php_ir::module::ClassEntry {
        id: ClassId::new(u32::MAX),
        name: "roundingmode".to_owned(),
        display_name: "RoundingMode".to_owned(),
        parent: None,
        parent_display_name: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: [
            "HalfAwayFromZero",
            "HalfTowardsZero",
            "HalfEven",
            "HalfOdd",
            "TowardsZero",
            "AwayFromZero",
            "NegativeInfinity",
            "PositiveInfinity",
        ]
        .into_iter()
        .map(|name| php_ir::module::ClassEnumCaseEntry {
            name: name.to_owned(),
            value: None,
            attributes: Vec::new(),
        })
        .collect(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor: None,
        flags: php_ir::module::ClassFlags {
            is_enum: true,
            ..php_ir::module::ClassFlags::default()
        },
        span: IrSpan::default(),
    }
}
