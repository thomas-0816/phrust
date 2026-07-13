use super::prelude::*;

#[derive(Clone, Debug)]
pub(super) struct VmCompileError {
    pub(super) message: String,
    diagnostic: Option<RuntimeDiagnostic>,
}

impl VmCompileError {
    pub(super) fn new(message: impl Into<String>) -> Box<Self> {
        Box::new(Self {
            message: message.into(),
            diagnostic: None,
        })
    }

    pub(super) fn typed(
        compiled: &CompiledUnit,
        payload: VmCompileDiagnostic,
        span: IrSpan,
    ) -> Box<Self> {
        let message = payload.status_message();
        let diagnostic = RuntimeDiagnostic::with_payload(
            payload.id(),
            RuntimeSeverity::FatalError,
            message.clone(),
            runtime_source_span(compiled, span),
            Vec::new(),
            Some(php_runtime::api::PhpReferenceClassification::FatalError),
            RuntimeDiagnosticPayload::VmCompile(payload),
        );
        Box::new(Self {
            message,
            diagnostic: Some(diagnostic),
        })
    }

    pub(super) fn into_parts(self) -> (String, Option<RuntimeDiagnostic>) {
        (self.message, self.diagnostic)
    }
}

impl From<String> for Box<VmCompileError> {
    fn from(message: String) -> Self {
        VmCompileError::new(message)
    }
}

impl From<&str> for Box<VmCompileError> {
    fn from(message: &str) -> Self {
        VmCompileError::new(message)
    }
}

pub(super) fn validate_class_table(compiled: &CompiledUnit) -> Result<(), Box<VmCompileError>> {
    for class in compiled.class_table() {
        if class.flags.is_final && class.flags.is_abstract {
            return Err(format!(
                "E_PHP_VM_INVALID_CLASS_MODIFIER: class {} cannot be both abstract and final",
                class.name
            )
            .into());
        }
        if class.flags.is_interface {
            validate_interface_declaration(compiled, class)?;
            for interface in &class.interfaces {
                let Some(parent) = compiled.lookup_class(interface) else {
                    if is_internal_interface(interface) {
                        continue;
                    }
                    if internal_class_kind(interface).is_some() {
                        return Err(format!(
                            "E_PHP_VM_INTERFACE_EXTENDS_CLASS: interface {} cannot extend non-interface {}",
                            class.name, interface
                        )
                        .into());
                    }
                    continue;
                };
                if !parent.flags.is_interface {
                    return Err(format!(
                        "E_PHP_VM_INTERFACE_EXTENDS_CLASS: interface {} cannot extend non-interface {}",
                        class.name, interface
                    )
                    .into());
                }
            }
            continue;
        }

        if let Some(parent_name) = class.parent.as_deref() {
            let parent_owned;
            let parent = if let Some(parent) = compiled.lookup_class(parent_name) {
                parent
            } else if let Some(internal_parent) = internal_class_table_validation_entry(parent_name)
            {
                parent_owned = internal_parent;
                &parent_owned
            } else {
                continue;
            };
            if parent.flags.is_interface {
                return Err(VmCompileError::typed(
                    compiled,
                    VmCompileDiagnostic::ClassExtendsInterface {
                        class_name: class.display_name.clone(),
                        interface_name: parent.display_name.clone(),
                    },
                    class.span,
                ));
            }
            if parent.flags.is_final {
                return Err(VmCompileError::typed(
                    compiled,
                    VmCompileDiagnostic::FinalClassExtend {
                        class_name: class.name.clone(),
                        parent_class_name: parent.name.clone(),
                    },
                    class.span,
                ));
            }
            validate_final_method_overrides(compiled, class, parent)?;
            validate_parent_method_compatibility(compiled, class, parent)?;
            validate_parent_property_compatibility(compiled, class, parent)?;
            validate_parent_constant_compatibility(compiled, class, parent)?;
        }

        validate_traversable_direct_implementation(compiled, class)?;

        for interface in &class.interfaces {
            let Some(interface_class) = compiled.lookup_class(interface) else {
                if is_internal_interface(interface) {
                    validate_internal_interface_implementation(compiled, class, interface)?;
                    continue;
                }
                if internal_class_kind(interface).is_some() {
                    return Err(VmCompileError::typed(
                        compiled,
                        VmCompileDiagnostic::ImplementsNonInterface {
                            class_name: class.name.clone(),
                            target_name: interface.clone(),
                            message: format!(
                                "class {} implements non-interface {}",
                                class.name, interface
                            ),
                        },
                        class.span,
                    ));
                }
                continue;
            };
            if !interface_class.flags.is_interface {
                return Err(VmCompileError::typed(
                    compiled,
                    VmCompileDiagnostic::ImplementsNonInterface {
                        class_name: class.display_name.clone(),
                        target_name: interface_class.display_name.clone(),
                        message: format!(
                            "{} cannot implement {} - it is not an interface",
                            class.display_name, interface_class.display_name
                        ),
                    },
                    class.span,
                ));
            }
            validate_interface_implementation(compiled, class, interface_class)?;
        }

        if !class.flags.is_abstract {
            validate_inherited_interface_implementations(compiled, class)?;
            validate_no_unimplemented_abstract_methods(compiled, class)?;
        }
    }
    Ok(())
}

pub(super) fn validate_traversable_direct_implementation(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
) -> Result<(), Box<VmCompileError>> {
    if !class
        .interfaces
        .iter()
        .any(|interface| normalize_class_name(interface) == "traversable")
    {
        return Ok(());
    }

    if class_implements_interface(compiled, &class.name, "Iterator", &mut Vec::new())?
        || class_implements_interface(compiled, &class.name, "IteratorAggregate", &mut Vec::new())?
    {
        return Ok(());
    }

    Err(VmCompileError::typed(
        compiled,
        VmCompileDiagnostic::TraversableDirectImplementation {
            class_name: class.display_name.clone(),
        },
        class.span,
    ))
}

pub(super) fn validate_interface_declaration(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
) -> Result<(), Box<VmCompileError>> {
    for constant in &class.constants {
        if constant.flags.is_private || constant.flags.is_protected {
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::InterfaceConstantVisibility {
                    class_name: class.display_name.clone(),
                    constant_name: constant.name.clone(),
                },
                constant.span,
            ));
        }
    }
    for method in &class.methods {
        if method.flags.is_private || method.flags.is_protected {
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::InterfaceMethodVisibility {
                    class_name: class.display_name.clone(),
                    method_name: method.name.clone(),
                },
                method_source_span(compiled, method),
            ));
        }
        if method.flags.has_body {
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::InterfaceMethodBody {
                    class_name: class.display_name.clone(),
                    method_name: method.name.clone(),
                },
                method_source_span(compiled, method),
            ));
        }
    }
    for property in &class.properties {
        if property.hooks.get.is_none() && property.hooks.set.is_none() {
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::InterfaceProperty {
                    class_name: class.display_name.clone(),
                    property_name: property.name.clone(),
                },
                class.span,
            ));
        }
    }
    Ok(())
}

pub(super) fn emit_private_final_method_warnings(
    compiled: &CompiledUnit,
    output: &mut OutputBuffer,
    state: &mut ExecutionState,
) {
    for class in compiled.class_table() {
        for method in &class.methods {
            if !method.flags.is_private
                || !method.flags.is_final
                || normalize_method_name(&method.name) == "__construct"
            {
                continue;
            }
            let source_span = compiled
                .unit()
                .functions
                .get(method.function.index())
                .map(|function| runtime_source_span(compiled, function.span))
                .unwrap_or_default();
            let diagnostic = RuntimeDiagnostic::new(
                "E_PHP_VM_PRIVATE_FINAL_METHOD_WARNING",
                RuntimeSeverity::Warning,
                "Private methods cannot be final as they are never overridden by other classes",
                source_span,
                Vec::new(),
                Some(php_runtime::api::PhpReferenceClassification::Warning),
            );
            if error_reporting_allows(state, php_runtime::api::PHP_E_WARNING) {
                emit_vm_diagnostic(
                    output,
                    state,
                    &diagnostic,
                    php_runtime::api::PhpDiagnosticChannel::Warning,
                    php_runtime::api::PHP_E_WARNING,
                );
                state.diagnostics.push(diagnostic);
            }
        }
    }
}

pub(super) fn emit_serializable_interface_deprecations(
    compiled: &CompiledUnit,
    output: &mut OutputBuffer,
    state: &mut ExecutionState,
) {
    for class in compiled.class_table() {
        if class.flags.is_interface || class.flags.is_trait {
            continue;
        }
        let implements_serializable = class.interfaces.iter().any(|interface| {
            interface_or_extends(
                compiled,
                interface,
                &normalize_class_name("Serializable"),
                &mut Vec::new(),
            )
            .unwrap_or(false)
        });
        if !implements_serializable
            || !error_reporting_allows(state, php_runtime::api::PHP_E_DEPRECATED)
        {
            continue;
        }
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_VM_SERIALIZABLE_INTERFACE_DEPRECATED",
            RuntimeSeverity::Deprecation,
            format!(
                "{} implements the Serializable interface, which is deprecated. Implement __serialize() and __unserialize() instead (or in addition, if support for old PHP versions is necessary)",
                class.display_name
            ),
            runtime_source_span(compiled, class.span),
            Vec::new(),
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
}

pub(super) fn internal_class_kind(class_name: &str) -> Option<php_std::ClassKind> {
    php_std::ExtensionRegistry::standard_library()
        .enabled_class(class_name)
        .map(php_std::ClassDescriptor::kind)
}

pub(super) fn is_internal_interface(class_name: &str) -> bool {
    internal_class_kind(class_name) == Some(php_std::ClassKind::Interface)
}

pub(super) fn internal_class_table_validation_entry(
    class_name: &str,
) -> Option<php_ir::module::ClassEntry> {
    let mut entry = internal_runtime_class_entry(&normalize_class_name(class_name))?;
    entry.parent = None;
    Some(entry)
}

pub(super) fn validate_internal_interface_implementation(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
    interface_name: &str,
) -> Result<(), Box<VmCompileError>> {
    if normalize_class_name(interface_name) == "datetimeinterface" {
        return Ok(());
    }
    for parent_name in internal_class_interfaces(interface_name) {
        validate_internal_interface_implementation(compiled, class, &parent_name)?;
    }
    for expected in php_std::generated::arginfo::class_methods(interface_name) {
        let Some(resolved) = lookup_method_in_hierarchy(compiled, class, expected.name, None)?
        else {
            if class_inherits_internal_runtime_method(compiled, class, expected.name)? {
                continue;
            }
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::InterfaceMethodMissing {
                    class_name: class.name.clone(),
                    interface_name: interface_name.to_owned(),
                    method_name: expected.name.to_owned(),
                },
                class.span,
            ));
        };
        if resolved.method.flags.is_abstract
            && class_inherits_internal_runtime_method(compiled, class, expected.name)?
        {
            continue;
        }
        if resolved.method.flags.is_private || resolved.method.flags.is_protected {
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::InterfaceMethodVisibility {
                    class_name: class.name.clone(),
                    method_name: expected.name.to_owned(),
                },
                method_source_span(compiled, resolved.method),
            ));
        }
    }
    Ok(())
}

pub(super) fn class_inherits_internal_runtime_method(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
    method: &str,
) -> Result<bool, String> {
    let mut current = class;
    let mut seen = Vec::new();
    loop {
        let Some(parent_name) = current.parent.as_deref() else {
            return Ok(false);
        };
        let normalized_parent = normalize_class_name(parent_name);
        if seen.iter().any(|name| name == &normalized_parent) {
            return Err(format!(
                "E_PHP_VM_CLASS_INHERITANCE_CYCLE: class {} participates in an inheritance cycle",
                class.name
            ));
        }
        seen.push(normalized_parent.clone());
        if internal_runtime_class_method_is_supported(&normalized_parent, method) {
            return Ok(true);
        }
        if internal_runtime_class_entry(&normalized_parent).is_some() {
            return Ok(false);
        }
        current = compiled.lookup_class(parent_name).ok_or_else(|| {
            format!(
                "E_PHP_VM_UNKNOWN_PARENT_CLASS: class {} extends missing class {}",
                current.name, parent_name
            )
        })?;
    }
}

pub(super) fn internal_runtime_class_method_is_supported(class_name: &str, method: &str) -> bool {
    let normalized = normalize_class_name(class_name);
    if is_spl_iterator_runtime_class(&normalized) {
        return spl_iterator_method_is_supported(method);
    }
    if is_spl_container_runtime_class(&normalized) {
        return spl_container_method_is_supported(method);
    }
    if is_spl_heap_runtime_class(&normalized) {
        return spl_heap_method_is_supported(method);
    }
    if is_spl_file_runtime_class(&normalized) {
        return spl_file_method_is_supported(method);
    }
    false
}

pub(super) fn class_extends_datetime_implementation(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
) -> bool {
    let mut current = class;
    let mut seen = Vec::new();
    loop {
        let normalized = normalize_class_name(&current.name);
        if matches!(normalized.as_str(), "datetime" | "datetimeimmutable") {
            return true;
        }
        if seen.iter().any(|name| name == &normalized) {
            return false;
        }
        seen.push(normalized);
        let Some(parent) = current.parent.as_deref() else {
            return false;
        };
        let parent_normalized = normalize_class_name(parent);
        if matches!(parent_normalized.as_str(), "datetime" | "datetimeimmutable") {
            return true;
        }
        let Some(parent_class) = compiled.lookup_class(parent) else {
            return false;
        };
        current = parent_class;
    }
}

pub(super) fn validate_final_method_overrides(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
    parent: &php_ir::module::ClassEntry,
) -> Result<(), Box<VmCompileError>> {
    for method in &class.methods {
        if let Some(parent_method) =
            lookup_method_in_hierarchy(compiled, parent, &method.name, None)?
            && parent_method.method.flags.is_final
            && (!parent_method.method.flags.is_private
                || normalize_method_name(&parent_method.method.name) == "__construct")
        {
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::FinalMethodOverride {
                    class_name: class.display_name.clone(),
                    method_name: method.name.clone(),
                    parent_class_name: parent_method.class.display_name.clone(),
                },
                method_source_span(compiled, method),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_parent_method_compatibility(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
    parent: &php_ir::module::ClassEntry,
) -> Result<(), Box<VmCompileError>> {
    for method in &class.methods {
        let Some(parent_method) = lookup_method_in_hierarchy(compiled, parent, &method.name, None)?
        else {
            continue;
        };
        if parent_method.method.flags.is_private {
            continue;
        }
        if parent_method.method.flags.is_static && !method.flags.is_static {
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::StaticMethodOverride {
                    class_name: class.name.clone(),
                    method_name: method.name.clone(),
                    parent_class_name: parent_method.class.name.clone(),
                    parent_is_static: true,
                },
                method_source_span(compiled, method),
            ));
        }
        if !parent_method.method.flags.is_static && method.flags.is_static {
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::StaticMethodOverride {
                    class_name: class.name.clone(),
                    method_name: method.name.clone(),
                    parent_class_name: parent_method.class.name.clone(),
                    parent_is_static: false,
                },
                method_source_span(compiled, method),
            ));
        }
        if method_visibility_rank(method.flags) < method_visibility_rank(parent_method.method.flags)
        {
            let visibility = method_visibility_name(parent_method.method.flags);
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::MethodVisibilityOverride {
                    class_name: class.name.clone(),
                    method_name: method.name.clone(),
                    required_visibility: visibility.to_owned(),
                    parent_class_name: parent_method.class.name.clone(),
                    weaker_suffix: visibility_weaker_suffix(visibility).to_owned(),
                },
                method_source_span(compiled, method),
            ));
        }
        if !method_signature_compatible(compiled, parent_method.method, method) {
            let actual_signature = method_signature_display(compiled, method)
                .unwrap_or_else(|| format!("{}::{}()", class.display_name, method.name));
            let interface_contract = inherited_interface_method_contract(compiled, parent, method)?;
            let expected = interface_contract.unwrap_or(parent_method.method);
            let expected_signature =
                method_signature_display(compiled, expected).unwrap_or_else(|| {
                    format!(
                        "{}::{}()",
                        parent_method.class.display_name, parent_method.method.name
                    )
                });
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::MethodSignatureOverride {
                    class_name: class.display_name.clone(),
                    method_name: method.name.clone(),
                    actual_signature,
                    expected_signature,
                },
                method_source_span(compiled, method),
            ));
        }
    }
    Ok(())
}

pub(super) fn inherited_interface_method_contract<'a>(
    compiled: &'a CompiledUnit,
    parent: &'a php_ir::module::ClassEntry,
    method: &php_ir::module::ClassMethodEntry,
) -> Result<Option<&'a php_ir::module::ClassMethodEntry>, String> {
    let mut lineage = Vec::new();
    collect_class_lineage_compiled(compiled, parent, &mut lineage)?;
    let mut seen = Vec::new();
    for declaring in lineage {
        for interface in &declaring.interfaces {
            if let Some(expected) =
                interface_method_contract(compiled, interface, &method.name, &mut seen)?
                && !method_signature_compatible(compiled, expected, method)
            {
                return Ok(Some(expected));
            }
        }
    }
    Ok(None)
}

pub(super) fn interface_method_contract<'a>(
    compiled: &'a CompiledUnit,
    interface_name: &str,
    method_name: &str,
    seen: &mut Vec<String>,
) -> Result<Option<&'a php_ir::module::ClassMethodEntry>, String> {
    let normalized = normalize_class_name(interface_name);
    if seen.iter().any(|name| name == &normalized) {
        return Ok(None);
    }
    seen.push(normalized);
    let Some(interface) = compiled.lookup_class(interface_name) else {
        return Ok(None);
    };
    for parent in &interface.interfaces {
        if let Some(method) = interface_method_contract(compiled, parent, method_name, seen)? {
            return Ok(Some(method));
        }
    }
    Ok(interface
        .methods
        .iter()
        .find(|method| method.name.eq_ignore_ascii_case(method_name)))
}

pub(super) fn method_visibility_rank(flags: php_ir::module::ClassMethodFlags) -> u8 {
    if flags.is_private {
        0
    } else if flags.is_protected {
        1
    } else {
        2
    }
}

pub(super) fn method_visibility_name(flags: php_ir::module::ClassMethodFlags) -> &'static str {
    if flags.is_private {
        "private"
    } else if flags.is_protected {
        "protected"
    } else {
        "public"
    }
}

pub(super) fn validate_parent_property_compatibility(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
    parent: &php_ir::module::ClassEntry,
) -> Result<(), Box<VmCompileError>> {
    for property in &class.properties {
        let Some(parent_property) =
            lookup_property_in_hierarchy(compiled, parent, &property.name, None)?
        else {
            continue;
        };
        if parent_property.property.flags.is_private {
            continue;
        }
        if parent_property.property.flags.is_static && !property.flags.is_static {
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::PropertyStaticOverride {
                    class_name: class.display_name.clone(),
                    property_name: property.name.clone(),
                    parent_class_name: parent_property.class.display_name.clone(),
                    parent_is_static: true,
                },
                class.span,
            ));
        }
        if !parent_property.property.flags.is_static && property.flags.is_static {
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::PropertyStaticOverride {
                    class_name: class.display_name.clone(),
                    property_name: property.name.clone(),
                    parent_class_name: parent_property.class.display_name.clone(),
                    parent_is_static: false,
                },
                class.span,
            ));
        }
        if property_visibility_rank(property.flags)
            < property_visibility_rank(parent_property.property.flags)
        {
            let visibility = property_visibility_name(parent_property.property.flags);
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::PropertyVisibilityOverride {
                    class_name: class.display_name.clone(),
                    property_name: property.name.clone(),
                    required_visibility: visibility.to_owned(),
                    parent_class_name: parent_property.class.display_name.clone(),
                    weaker_suffix: visibility_weaker_suffix(visibility).to_owned(),
                },
                class.span,
            ));
        }
    }
    Ok(())
}

pub(super) fn property_visibility_rank(flags: php_ir::module::ClassPropertyFlags) -> u8 {
    if flags.is_private {
        0
    } else if flags.is_protected {
        1
    } else {
        2
    }
}

pub(super) fn property_visibility_name(flags: php_ir::module::ClassPropertyFlags) -> &'static str {
    if flags.is_private {
        "private"
    } else if flags.is_protected {
        "protected"
    } else {
        "public"
    }
}

pub(super) fn validate_parent_constant_compatibility(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
    parent: &php_ir::module::ClassEntry,
) -> Result<(), Box<VmCompileError>> {
    for constant in &class.constants {
        let Some(parent_constant) =
            lookup_constant_in_hierarchy(compiled, parent, &constant.name, None)?
        else {
            continue;
        };
        if parent_constant.constant.flags.is_private {
            continue;
        }
        if constant_visibility_rank(constant.flags)
            < constant_visibility_rank(parent_constant.constant.flags)
        {
            let visibility = constant_visibility_name(parent_constant.constant.flags);
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::ClassConstantVisibilityOverride {
                    class_name: class.display_name.clone(),
                    constant_name: constant.name.clone(),
                    required_visibility: visibility.to_owned(),
                    parent_class_name: parent_constant.class.display_name.clone(),
                    weaker_suffix: visibility_weaker_suffix(visibility).to_owned(),
                },
                class.span,
            ));
        }
    }
    Ok(())
}

pub(super) fn constant_visibility_rank(flags: php_ir::module::ClassConstantFlags) -> u8 {
    if flags.is_private {
        0
    } else if flags.is_protected {
        1
    } else {
        2
    }
}

pub(super) fn constant_visibility_name(flags: php_ir::module::ClassConstantFlags) -> &'static str {
    if flags.is_private {
        "private"
    } else if flags.is_protected {
        "protected"
    } else {
        "public"
    }
}

pub(super) fn visibility_weaker_suffix(visibility: &str) -> &'static str {
    if visibility == "protected" {
        " or weaker"
    } else {
        ""
    }
}

pub(super) fn method_signature_display(
    compiled: &CompiledUnit,
    method: &php_ir::module::ClassMethodEntry,
) -> Option<String> {
    let function = compiled.unit().functions.get(method.function.index())?;
    let params = function
        .params
        .iter()
        .map(method_param_display)
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!("{}({params})", function.name))
}

pub(super) fn method_source_span(
    compiled: &CompiledUnit,
    method: &php_ir::module::ClassMethodEntry,
) -> IrSpan {
    compiled
        .unit()
        .functions
        .get(method.function.index())
        .map_or(IrSpan::default(), |function| function.span)
}

pub(super) fn method_param_display(param: &php_ir::IrParam) -> String {
    let mut out = String::new();
    if let Some(type_) = &param.type_ {
        out.push_str(&method_type_display(type_));
        out.push(' ');
    }
    if param.by_ref {
        out.push('&');
    }
    if param.variadic {
        out.push_str("...");
    }
    out.push('$');
    out.push_str(&param.name);
    if let Some(default) = &param.default {
        out.push_str(" = ");
        out.push_str(&method_default_display(default));
    }
    out
}

pub(super) fn method_default_display(default: &IrConstant) -> String {
    match default {
        IrConstant::Null => "NULL".to_owned(),
        IrConstant::Bool(true) => "true".to_owned(),
        IrConstant::Bool(false) => "false".to_owned(),
        IrConstant::Int(value) => value.to_string(),
        IrConstant::Float(value) => {
            let mut display = value.to_string();
            if !display.contains('.') && !display.contains('e') && !display.contains('E') {
                display.push_str(".0");
            }
            display
        }
        IrConstant::String(value) => {
            format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
        }
        IrConstant::StringBytes(value) => {
            let escaped = value
                .iter()
                .map(|byte| match byte {
                    b'\\' => "\\\\".to_owned(),
                    b'\'' => "\\'".to_owned(),
                    0x20..=0x7e => char::from(*byte).to_string(),
                    _ => format!("\\x{byte:02x}"),
                })
                .collect::<String>();
            format!("'{escaped}'")
        }
        IrConstant::NamedConstant(name) => name.clone(),
        IrConstant::ClassConstant {
            class_name,
            constant_name,
        } => format!("{class_name}::{constant_name}"),
        IrConstant::Array(_) => "array".to_owned(),
    }
}

pub(super) fn method_type_display(type_: &IrReturnType) -> String {
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
        IrReturnType::Nullable { inner } => format!("?{}", method_type_display(inner)),
        IrReturnType::Union { members } => members
            .iter()
            .map(method_type_display)
            .collect::<Vec<_>>()
            .join("|"),
        IrReturnType::Intersection { members } => members
            .iter()
            .map(method_type_display)
            .collect::<Vec<_>>()
            .join("&"),
        IrReturnType::Dnf { members } => members
            .iter()
            .map(method_type_display)
            .collect::<Vec<_>>()
            .join("|"),
    }
}

pub(super) fn validate_no_unimplemented_abstract_methods(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
) -> Result<(), Box<VmCompileError>> {
    let mut lineage = Vec::new();
    collect_class_lineage_compiled(compiled, class, &mut lineage)?;
    for declaring in lineage {
        for method in &declaring.methods {
            if !method.flags.is_abstract {
                continue;
            }
            let resolved = lookup_method_in_hierarchy(compiled, class, &method.name, None)?
                .ok_or_else(|| {
                    VmCompileError::new(format!(
                        "E_PHP_VM_ABSTRACT_METHOD_NOT_IMPLEMENTED: class {} does not implement {}::{}",
                        class.name, declaring.name, method.name
                    ))
                })?;
            if resolved.method.flags.is_abstract {
                return Err(format!(
                    "E_PHP_VM_ABSTRACT_METHOD_NOT_IMPLEMENTED: class {} does not implement {}::{}",
                    class.name, declaring.name, method.name
                )
                .into());
            }
        }
    }
    Ok(())
}

pub(super) fn validate_interface_implementation(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
    interface: &php_ir::module::ClassEntry,
) -> Result<(), Box<VmCompileError>> {
    for parent_name in &interface.interfaces {
        let Some(parent) = compiled.lookup_class(parent_name) else {
            if is_internal_interface(parent_name) {
                validate_internal_interface_implementation(compiled, class, parent_name)?;
                continue;
            }
            continue;
        };
        validate_interface_implementation(compiled, class, parent)?;
    }
    for expected in &interface.methods {
        let Some(resolved) = lookup_method_in_hierarchy(compiled, class, &expected.name, None)?
        else {
            if class.flags.is_abstract {
                continue;
            }
            let method_name = method_display_name(compiled, expected);
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::InterfaceMethodMissing {
                    class_name: class.display_name.clone(),
                    interface_name: interface.display_name.clone(),
                    method_name,
                },
                class.span,
            ));
        };
        if resolved.method.flags.is_private || resolved.method.flags.is_protected {
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::InterfaceMethodVisibility {
                    class_name: class.name.clone(),
                    method_name: expected.name.clone(),
                },
                method_source_span(compiled, resolved.method),
            ));
        }
        if !method_signature_compatible(compiled, expected, resolved.method) {
            let actual_signature = method_signature_display(compiled, resolved.method)
                .unwrap_or_else(|| format!("{}::{}()", class.display_name, resolved.method.name));
            let expected_signature = method_signature_display(compiled, expected)
                .unwrap_or_else(|| format!("{}::{}()", interface.display_name, expected.name));
            return Err(VmCompileError::typed(
                compiled,
                VmCompileDiagnostic::InterfaceMethodSignature {
                    class_name: class.display_name.clone(),
                    method_name: resolved.method.name.clone(),
                    actual_signature,
                    expected_signature,
                },
                method_source_span(compiled, resolved.method),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_inherited_interface_implementations(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
) -> Result<(), Box<VmCompileError>> {
    let mut lineage = Vec::new();
    collect_class_lineage_compiled(compiled, class, &mut lineage)?;
    let mut seen = Vec::new();
    for declaring in lineage {
        for interface in &declaring.interfaces {
            let normalized = normalize_class_name(interface);
            if seen.iter().any(|name| name == &normalized) {
                continue;
            }
            seen.push(normalized);
            let Some(interface_class) = compiled.lookup_class(interface) else {
                if is_internal_interface(interface) {
                    validate_internal_interface_implementation(compiled, class, interface)?;
                    continue;
                }
                continue;
            };
            if interface_class.flags.is_interface {
                validate_interface_implementation(compiled, class, interface_class)?;
            }
        }
    }
    Ok(())
}

pub(super) fn method_signature_compatible(
    compiled: &CompiledUnit,
    expected: &php_ir::module::ClassMethodEntry,
    actual: &php_ir::module::ClassMethodEntry,
) -> bool {
    let Some(expected_fn) = compiled.unit().functions.get(expected.function.index()) else {
        return false;
    };
    let Some(actual_fn) = compiled.unit().functions.get(actual.function.index()) else {
        return false;
    };
    if expected_fn.returns_by_ref != actual_fn.returns_by_ref
        || expected_fn.return_type != actual_fn.return_type
    {
        return false;
    }
    if actual_fn.params.len() < expected_fn.params.len() {
        return false;
    }
    if actual_fn.params[expected_fn.params.len()..]
        .iter()
        .any(|param| param.required)
    {
        return false;
    }
    expected_fn
        .params
        .iter()
        .zip(&actual_fn.params)
        .all(|(expected_param, actual_param)| {
            (!actual_param.required || expected_param.required)
                && expected_param.type_ == actual_param.type_
                && expected_param.by_ref == actual_param.by_ref
                && expected_param.variadic == actual_param.variadic
        })
}

pub(super) fn validate_object_mvp(class: &RuntimeClassEntry) -> Result<(), String> {
    validate_object_mvp_with_display_name(class, &class.name)
}

pub(super) fn validate_object_mvp_with_display_name(
    class: &RuntimeClassEntry,
    display_name: &str,
) -> Result<(), String> {
    if class.flags.is_enum {
        return Err(format!(
            "E_PHP_VM_ENUM_INSTANTIATION: enum {} cannot be instantiated",
            display_name
        ));
    }
    if class.flags.is_interface {
        return Err(format!(
            "E_PHP_VM_INTERFACE_INSTANTIATION: Cannot instantiate interface {}",
            display_name
        ));
    }
    if class.flags.is_abstract {
        return Err(format!(
            "E_PHP_VM_ABSTRACT_CLASS_INSTANTIATION: Cannot instantiate abstract class {}",
            display_name
        ));
    }
    for method in &class.methods {
        if method.flags.is_abstract {
            return Err(format!(
                "E_PHP_VM_UNSUPPORTED_METHOD_MODIFIER: method {}::{} is abstract outside the concrete-method concrete method MVP",
                class.name, method.name
            ));
        }
    }
    Ok(())
}
