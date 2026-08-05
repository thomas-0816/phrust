//! Cold request/publication-time native metadata construction.
//!
//! Class layouts and immutable constant relocations are resolved once
//! before generated code executes. Per-invocation validation is forbidden.

use super::*;

pub(super) fn publish_native_symbol_query(
    context: &NativeRequestColdState<'_>,
) -> NativeSymbolQueryCapability {
    NativeSymbolQueryCapability {
        active_compiled: std::ptr::from_ref(&context.compiled),
        current_dynamic_unit: std::ptr::from_ref(&context.current_dynamic_unit),
        dynamic_units: std::ptr::from_ref(&context.dynamic_units),
        dynamic_functions: std::ptr::from_ref(&context.dynamic_functions),
        external_functions: std::ptr::from_ref(&context.external_functions),
        external_class_units: std::ptr::from_ref(&context.external_class_units),
        deployment_functions: std::ptr::from_ref(&context.deployment_functions),
        deployment_classes: std::ptr::from_ref(&context.deployment_classes),
        visible_function_names: std::ptr::from_ref(&context.visible_function_names),
        native_dynamic_constants: std::ptr::from_ref(&context.native_dynamic_constants)
            as *mut std::collections::BTreeMap<String, i64>,
        trusted_dynamic_constant_sites: std::ptr::from_ref(&context.trusted_dynamic_constant_sites),
        dynamic_classes: std::ptr::from_ref(&context.dynamic_classes),
        class_aliases: std::ptr::from_ref(&context.class_aliases),
    }
}

pub(super) fn publish_native_request_query(
    context: &NativeRequestColdState<'_>,
) -> NativeRequestQueryCapability {
    NativeRequestQueryCapability {
        environment: std::ptr::from_ref(&context.environment),
        included_files: std::ptr::from_ref(&context.included_files),
        sapi_name: std::ptr::from_ref(&context.options.runtime_context.sapi_name),
    }
}

pub(super) fn publish_native_configuration(
    context: &NativeRequestColdState<'_>,
) -> NativeConfigurationCapability {
    NativeConfigurationCapability {
        ini_registry: std::ptr::from_ref(&context.ini_registry)
            as *mut php_runtime::api::IniRegistry,
        include_path: std::ptr::from_ref(&context.include_path)
            as *mut Arc<Vec<std::path::PathBuf>>,
        display_errors: std::ptr::from_ref(&context.display_errors) as *mut bool,
        error_reporting: std::ptr::from_ref(&context.error_reporting) as *mut i64,
        default_timezone: std::ptr::from_ref(&context.default_timezone) as *mut String,
    }
}

pub(super) fn publish_native_http_response(
    context: &NativeRequestColdState<'_>,
) -> NativeHttpResponseCapability {
    NativeHttpResponseCapability {
        response: std::ptr::from_ref(&context.http_response)
            as *mut php_runtime::api::RuntimeHttpResponseState,
    }
}

pub(super) fn publish_native_execution_deadline(
    context: &mut NativeRequestColdState<'_>,
) -> NativeExecutionDeadlineCapability {
    NativeExecutionDeadlineCapability {
        deadline: std::ptr::from_mut(&mut context.execution_deadline_at),
        mutable: u8::from(context.execution_deadline_mutable),
        diagnostic: std::ptr::from_mut(&mut context.diagnostic),
    }
}

pub(super) fn publish_native_runtime_diagnostic(
    context: &mut NativeRequestColdState<'_>,
) -> NativeRuntimeDiagnosticCapability {
    NativeRuntimeDiagnosticCapability {
        diagnostic: std::ptr::from_mut(&mut context.diagnostic),
    }
}

pub(super) fn publish_native_frame_arena(
    context: &mut NativeRequestColdState<'_>,
) -> NativeFrameArenaCapability {
    NativeFrameArenaCapability {
        arena: std::ptr::from_mut(&mut context.native_frame_arena),
        diagnostic: std::ptr::from_mut(&mut context.diagnostic),
    }
}
use php_runtime::api::PhpString;
use php_runtime::api::Value;

impl<'a> NativeRequestColdState<'a> {
    /// Resolve immutable local class layouts once for the active source unit.
    /// Plans with request-dependent defaults or unresolved external parents
    /// remain empty and retain their single baseline continuation.
    pub(super) fn prepare_trusted_class_plans(&mut self) {
        if self.trusted_class_plans.len() == self.unit.classes.len()
            && self.unit.classes.iter().enumerate().all(|(index, class)| {
                !native_class_is_publication_allocatable(self, self.current_dynamic_unit, class)
                    || self.trusted_class_plans[index].state
                        == php_jit::JIT_NATIVE_PREPARED_CLASS_ALLOCATABLE
            })
        {
            return;
        }
        let owner = self.current_dynamic_unit;
        let classes = self.unit.classes.clone();
        if self.trusted_class_plans.len() != classes.len() {
            self.trusted_class_plans.resize(
                classes.len(),
                php_jit::JitNativePreparedClassPlan::default(),
            );
        }
        for (index, class) in classes.iter().enumerate() {
            if self.trusted_class_plans[index].state
                == php_jit::JIT_NATIVE_PREPARED_CLASS_ALLOCATABLE
            {
                continue;
            }
            if !native_class_is_publication_allocatable(self, owner, class) {
                continue;
            }
            let key = (owner, class.name.clone());
            let cached = { self.runtime_class_cache.borrow().get(&key).cloned() };
            let prepared = if let Some(cached) = cached {
                Some(cached)
            } else {
                let Ok(entry) = native_runtime_class_with_owner(self, owner, class) else {
                    continue;
                };
                let default_declared_slots = php_runtime::api::ObjectRef::default_declared_slots(
                    &entry,
                    &class.display_name,
                );
                let mut owned_defaults = Vec::new();
                let mut default_native_slots = Vec::with_capacity(default_declared_slots.len());
                let mut failed = false;
                for default in default_declared_slots {
                    let encoded = match default {
                        None => {
                            default_native_slots
                                .push(php_runtime::api::NativeDeclaredPropertySlot::default());
                            continue;
                        }
                        Some(Value::Uninitialized) => {
                            php_jit::jit_encode_constant(php_jit::JIT_VALUE_UNINITIALIZED)
                        }
                        Some(value) => match self.encode_baseline_value(value) {
                            Ok(encoded) => encoded,
                            Err(_) => {
                                failed = true;
                                break;
                            }
                        },
                    };
                    if let Some(runtime_index) = php_jit::jit_decode_runtime_value(encoded) {
                        if runtime_index < php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE {
                            let _ = self.release(encoded);
                            failed = true;
                            break;
                        }
                        owned_defaults.push(encoded);
                    }
                    default_native_slots.push(php_runtime::api::NativeDeclaredPropertySlot {
                        initialized: 1,
                        reserved: 0,
                        value: encoded,
                    });
                }
                if failed {
                    for encoded in owned_defaults {
                        let _ = self.release(encoded);
                    }
                    continue;
                }
                let layout_id =
                    php_runtime::api::ObjectRef::prepared_layout_id(&entry, &class.display_name);
                self.runtime_class_layout_cache
                    .borrow_mut()
                    .insert(key.clone(), layout_id);
                let prepared = Rc::new(PreparedNativeRuntimeClass {
                    entry,
                    display_name: class.display_name.clone(),
                    layout_id,
                    default_native_slots: default_native_slots.into_boxed_slice(),
                });
                self.runtime_class_cache
                    .borrow_mut()
                    .insert(key.clone(), Rc::clone(&prepared));
                Some(prepared)
            };
            let Some(prepared) = prepared else {
                continue;
            };
            self.trusted_class_plans[index] = php_jit::JitNativePreparedClassPlan {
                prepared: Rc::as_ptr(&prepared) as usize as u64,
                display_name_bytes: prepared.display_name.as_ptr() as usize as u64,
                display_name_length: prepared.display_name.len() as u64,
                state: php_jit::JIT_NATIVE_PREPARED_CLASS_ALLOCATABLE,
                flags: if prepared.entry.constructor_id.is_some()
                    || prepared
                        .entry
                        .methods
                        .iter()
                        .any(|method| method.name.eq_ignore_ascii_case("__construct"))
                {
                    php_jit::JIT_NATIVE_PREPARED_CLASS_HAS_CONSTRUCTOR
                } else {
                    0
                },
            };
        }
    }

    /// Resolve immutable constant sites at the request/publication boundary.
    /// A namespace fallback is deliberately not cached: defining the primary
    /// name later in the request must change subsequent lookup. Class
    /// constants are published only when resolution is effect-free and
    /// independent of the late-static calling class.
    pub(super) fn prepare_trusted_constant_fetches(&mut self) {
        // Build exact publication sites in one pass. The former name set fed
        // every distinct name back into `publish_trusted_constant_name`,
        // which rescanned every function and continuation for each name.
        let mut sites = std::collections::BTreeMap::<String, Vec<(u32, u32)>>::new();
        let mut class_sites = Vec::<(u32, u32, String, String)>::new();
        for function in self.published_native_functions() {
            let Some(instructions) = self.prepared_continuation_instructions(function) else {
                continue;
            };
            let function = function.raw();
            for (continuation, instruction) in instructions.iter().enumerate() {
                let Some(instruction) = instruction.as_ref() else {
                    continue;
                };
                let Ok(continuation) = u32::try_from(continuation) else {
                    continue;
                };
                match &instruction.kind {
                    php_ir::InstructionKind::FetchConst { name, .. } => {
                        sites
                            .entry(name.clone())
                            .or_default()
                            .push((function, continuation));
                        if let Some(index) = self
                            .trusted_property_function_offsets
                            .get(function as usize)
                            .and_then(|base| usize::try_from(*base).ok())
                            .and_then(|base| base.checked_add(continuation as usize))
                        {
                            let published = self
                                .trusted_dynamic_constant_sites
                                .entry(name.clone())
                                .or_default();
                            if !published.contains(&index) {
                                published.push(index);
                            }
                        }
                    }
                    php_ir::InstructionKind::FetchClassConstant {
                        class_name,
                        constant,
                        ..
                    } => class_sites.push((
                        function,
                        continuation,
                        class_name.clone(),
                        constant.clone(),
                    )),
                    _ => {}
                }
            }
        }
        for (name, sites) in sites {
            match self.encode_named_runtime_constant_owned(&name, 0) {
                Ok(encoded) => {
                    for (function, continuation) in sites {
                        let _ =
                            self.publish_trusted_constant_fetch(function, continuation, encoded);
                    }
                    let _ = self.release(encoded);
                }
                Err(_) => {
                    for (function, continuation) in sites {
                        let Some(base) = self
                            .trusted_property_function_offsets
                            .get(function as usize)
                            .copied()
                            .and_then(|base| usize::try_from(base).ok())
                        else {
                            continue;
                        };
                        let index = base.saturating_add(continuation as usize);
                        let function_id = php_ir::FunctionId::new(function);
                        let Some(span) = self
                            .prepared_continuation_instructions(function_id)
                            .and_then(|instructions| {
                                instructions
                                    .get(continuation as usize)
                                    .and_then(Option::as_ref)
                                    .map(|instruction| instruction.span)
                            })
                        else {
                            continue;
                        };
                        let (function_name, include_function_frame) =
                            self.unit.functions.get(function as usize).map_or_else(
                                || ("{main}".to_owned(), false),
                                |function| (function.name.clone(), !function.flags.is_top_level),
                            );
                        let owner = PreparedNativeThrowableOwner::UndefinedConstant(Box::new(
                            PreparedNativeUndefinedConstantContract {
                                throwable: prepare_native_throwable_site(
                                    self,
                                    "Error",
                                    &function_name,
                                    include_function_frame,
                                    span,
                                ),
                                message: format!("Undefined constant \"{name}\""),
                            },
                        ));
                        let pointer = owner.undefined_constant_pointer().unwrap_or(0);
                        self.trusted_exception_plan_owners.insert(index, owner);
                        if let Some(plan) = self.trusted_constant_slots.get_mut(index) {
                            *plan = php_jit::JitNativeTrustedConstantSlot {
                                value: pointer as i64,
                                state: php_jit::JIT_NATIVE_TRUSTED_CONSTANT_ERROR,
                                reserved: 0,
                            };
                        }
                    }
                }
            }
        }
        for (function, continuation, class_name, constant) in class_sites {
            let Some(encoded) =
                self.prepare_effect_free_class_constant_owned(function, &class_name, &constant)
            else {
                if self.class_constant_is_proven_missing(function, &class_name, &constant) {
                    let Some(base) = self
                        .trusted_property_function_offsets
                        .get(function as usize)
                        .copied()
                        .and_then(|base| usize::try_from(base).ok())
                    else {
                        continue;
                    };
                    let index = base.saturating_add(continuation as usize);
                    let function_id = php_ir::FunctionId::new(function);
                    let Some(span) = self
                        .prepared_continuation_instructions(function_id)
                        .and_then(|instructions| {
                            instructions
                                .get(continuation as usize)
                                .and_then(Option::as_ref)
                                .map(|instruction| instruction.span)
                        })
                    else {
                        continue;
                    };
                    let (function_name, include_function_frame) =
                        self.unit.functions.get(function as usize).map_or_else(
                            || ("{main}".to_owned(), false),
                            |function| (function.name.clone(), !function.flags.is_top_level),
                        );
                    let owner = PreparedNativeThrowableOwner::UndefinedConstant(Box::new(
                        PreparedNativeUndefinedConstantContract {
                            throwable: prepare_native_throwable_site(
                                self,
                                "Error",
                                &function_name,
                                include_function_frame,
                                span,
                            ),
                            message: format!("Undefined constant {class_name}::{constant}"),
                        },
                    ));
                    let pointer = owner.undefined_constant_pointer().unwrap_or(0);
                    self.trusted_exception_plan_owners.insert(index, owner);
                    if let Some(plan) = self.trusted_constant_slots.get_mut(index) {
                        *plan = php_jit::JitNativeTrustedConstantSlot {
                            value: pointer as i64,
                            state: php_jit::JIT_NATIVE_TRUSTED_CONSTANT_ERROR,
                            reserved: 0,
                        };
                    }
                }
                continue;
            };
            let _ = self.publish_trusted_constant_fetch(function, continuation, encoded);
            let _ = self.release(encoded);
        }
    }

    fn class_constant_is_proven_missing(
        &self,
        caller_function: u32,
        class_name: &str,
        constant_name: &str,
    ) -> bool {
        if constant_name.eq_ignore_ascii_case("class") {
            return false;
        }
        let mut candidate = match class_name.to_ascii_lowercase().as_str() {
            "static" => return false,
            "self" => match native_effective_calling_class(self, caller_function) {
                Some(class) => class.name.clone(),
                None => return false,
            },
            "parent" => match native_effective_calling_class(self, caller_function)
                .and_then(|class| class.parent.clone())
            {
                Some(parent) => parent,
                None => return false,
            },
            _ => normalize_class_name(class_name),
        };
        if let Some(original) = self.class_aliases.get(&normalize_class_name(&candidate)) {
            candidate = original.clone();
        }
        loop {
            let class = if let Some(class) = native_active_class_handle(self, &candidate) {
                class
            } else if let Some((_, class)) = native_external_class_handle(self, &candidate) {
                class
            } else {
                return false;
            };
            if class
                .constants
                .iter()
                .any(|entry| entry.name.eq_ignore_ascii_case(constant_name))
                || class
                    .enum_cases
                    .iter()
                    .any(|case| case.name.eq_ignore_ascii_case(constant_name))
            {
                return false;
            }
            let Some(parent) = class.parent.as_deref() else {
                return true;
            };
            candidate = normalize_class_name(parent);
        }
    }

    /// Publishes only class constants whose lookup cannot autoload, diagnose,
    /// depend on visibility, or vary with late-static binding. More dynamic
    /// sites are populated by the completed baseline continuation after its
    /// PHP-visible effects have run.
    pub(super) fn prepare_effect_free_class_constant_owned(
        &mut self,
        caller_function: u32,
        class_name: &str,
        constant_name: &str,
    ) -> Option<i64> {
        fn is_direct_literal(constant: &php_ir::IrConstant) -> bool {
            match constant {
                php_ir::IrConstant::NamedConstant(_) | php_ir::IrConstant::ClassConstant { .. } => {
                    false
                }
                php_ir::IrConstant::Array(entries) => entries.iter().all(|entry| {
                    entry.key.as_ref().is_none_or(is_direct_literal)
                        && is_direct_literal(&entry.value)
                }),
                _ => true,
            }
        }

        let mut resolved_class = match class_name.to_ascii_lowercase().as_str() {
            "static" => return None,
            "self" => native_effective_calling_class(self, caller_function)?
                .name
                .clone(),
            "parent" => native_effective_calling_class(self, caller_function)?
                .parent
                .clone()?,
            _ => normalize_class_name(class_name),
        };
        if let Some(original) = self
            .class_aliases
            .get(&normalize_class_name(&resolved_class))
        {
            resolved_class = original.clone();
        }
        if constant_name.eq_ignore_ascii_case("class") {
            let display = native_active_class_handle(self, &resolved_class)
                .map(|class| class.display_name.clone())
                .or_else(|| {
                    native_external_class_handle(self, &resolved_class)
                        .map(|(_, class)| class.display_name.clone())
                })
                .unwrap_or(resolved_class);
            return self
                .encode_native_string_owner(PhpString::from_bytes(display.into_bytes()))
                .ok();
        }

        resolved_class = normalize_class_name(&resolved_class);
        if class_name.eq_ignore_ascii_case("ArrayObject")
            && constant_name.eq_ignore_ascii_case("ARRAY_AS_PROPS")
        {
            return Some(2);
        }
        if pdo_mysql_deprecated_constant(&resolved_class, constant_name).is_some() {
            return None;
        }
        if let Some(value) = native_internal_class_constant(&resolved_class, constant_name) {
            return self.encode_baseline_value(value).ok();
        }

        let mut candidate = resolved_class;
        loop {
            let (owner_unit, class) =
                if let Some(class) = native_active_class_handle(self, &candidate) {
                    (None, class)
                } else if let Some((unit, class)) = native_external_class_handle(self, &candidate) {
                    (Some(unit), class)
                } else {
                    return None;
                };
            if let Some(entry) = class
                .constants
                .iter()
                .find(|entry| entry.name.eq_ignore_ascii_case(constant_name))
            {
                if entry.flags.is_private || entry.flags.is_protected {
                    return None;
                }
                let constant = entry.value.and_then(|value| {
                    owner_unit.map_or_else(
                        || self.unit.constants.get(value.index()),
                        |unit| {
                            self.dynamic_units.get(unit).and_then(|package| {
                                package.compiled.unit().constants.get(value.index())
                            })
                        },
                    )
                })?;
                if !is_direct_literal(constant) {
                    return None;
                }
                let constant = constant.clone();
                return self.encode_native_ir_constant_owned(&constant).ok();
            }
            if class
                .enum_cases
                .iter()
                .any(|case| case.name.eq_ignore_ascii_case(constant_name))
            {
                return None;
            }
            candidate = normalize_class_name(class.parent.as_deref()?);
        }
    }
    /// Resolves a named constant into an independently owned native encoding.
    /// Compiled declarations and native `define()` values remain in their
    /// authoritative representation. Extension constants cross their one
    /// explicit cold boundary only when no compiled/native declaration
    /// exists.
    pub(super) fn encode_named_runtime_constant_owned(
        &mut self,
        name: &str,
        depth: usize,
    ) -> Result<i64, String> {
        if depth > 32 {
            return Err("native constant resolution exceeded its recursion limit".to_owned());
        }
        if let Some(encoded) = self.native_dynamic_constants.get(name).copied() {
            return self
                .duplicate_authoritative_native_value(encoded)?
                .ok_or_else(|| {
                    format!("native dynamic constant {name} is not authoritative native data")
                });
        }
        if let Some(constant) = self
            .unit
            .constant_table
            .iter()
            .find(|constant| constant.name == name)
            .and_then(|constant| self.unit.constants.get(constant.value.index()))
            .cloned()
        {
            return self.encode_native_ir_constant_owned_at_depth(&constant, depth + 1);
        }
        php_std::ExtensionRegistry::standard_library()
            .enabled_constant(name)
            .and_then(php_std::ConstantDescriptor::value)
            .map(php_std::constants::constant_to_value)
            .ok_or_else(|| format!("Undefined constant \"{name}\""))
            .and_then(|value| self.encode_baseline_value(value))
    }
}

pub(super) fn native_publication_constant_is_stable(constant: &php_ir::IrConstant) -> bool {
    match constant {
        php_ir::IrConstant::Null
        | php_ir::IrConstant::Bool(_)
        | php_ir::IrConstant::Int(_)
        | php_ir::IrConstant::Float(_)
        | php_ir::IrConstant::String(_)
        | php_ir::IrConstant::StringBytes(_) => true,
        php_ir::IrConstant::Array(entries) => entries.iter().all(|entry| {
            entry
                .key
                .as_ref()
                .is_none_or(native_publication_constant_is_stable)
                && native_publication_constant_is_stable(&entry.value)
        }),
        php_ir::IrConstant::NamedConstant(_) | php_ir::IrConstant::ClassConstant { .. } => false,
    }
}

pub(super) fn native_internal_class_is_available(class_name: &str) -> bool {
    php_std::ExtensionRegistry::standard_library()
        .enabled_class(class_name)
        .is_some()
        || matches!(
            class_name,
            "stdclass"
                | "exception"
                | "errorexception"
                | "error"
                | "typeerror"
                | "valueerror"
                | "argumentcounterror"
                | "fibererror"
                | "closure"
                | "generator"
                | "fiber"
                | "arrayobject"
                | "arrayiterator"
        )
}

pub(super) fn native_class_is_publication_allocatable(
    context: &NativeRequestColdState<'_>,
    owner_unit: Option<usize>,
    class: &php_ir::module::ClassEntry,
) -> bool {
    let owner_ir_unit = |owner: Option<usize>| -> Option<&php_ir::IrUnit> {
        match owner {
            None => Some(&*context.unit),
            Some(unit) if context.current_dynamic_unit == Some(unit) => Some(&*context.unit),
            Some(unit) => context
                .dynamic_units
                .get(unit)
                .map(|package| package.compiled.unit()),
        }
    };
    if class.flags.is_abstract
        || class.flags.is_interface
        || class.flags.is_trait
        || class.flags.is_enum
    {
        return false;
    }
    let mut current = Some((owner_unit, class));
    let mut visited = std::collections::BTreeSet::new();
    while let Some((owner, candidate)) = current {
        // An abstract ancestor is a valid layout contributor for a concrete
        // child. Only the class being allocated is forbidden from being
        // abstract; non-class ancestors remain invalid throughout lineage.
        if candidate.flags.is_interface
            || candidate.flags.is_trait
            || candidate.flags.is_enum
            || !visited.insert((owner, candidate.name.as_str()))
        {
            return false;
        }
        let Some(constants) = owner_ir_unit(owner).map(|unit| unit.constants.as_slice()) else {
            return false;
        };
        if candidate.properties.iter().any(|property| {
            property
                .default
                .and_then(|constant| constants.get(constant.index()))
                .is_some_and(|constant| !native_publication_constant_is_stable(constant))
        }) {
            return false;
        }
        current = match candidate.parent.as_deref() {
            None => None,
            Some(parent) => {
                let parent = normalize_class_name(parent);
                if let Some(parent) = owner_ir_unit(owner)
                    .into_iter()
                    .flat_map(|unit| &unit.classes)
                    .find(|class| class.name == parent)
                {
                    Some((owner, parent))
                } else if let Some((unit, parent)) = native_external_class_ref(context, &parent) {
                    Some((Some(unit), parent))
                } else {
                    if !native_internal_class_is_available(&parent) {
                        return false;
                    }
                    None
                }
            }
        };
    }
    true
}

pub(super) struct NativeStaticPropertyDeclaration {
    pub(super) owner_unit: Option<usize>,
    pub(super) owner_name: String,
    pub(super) owner_display_name: String,
    pub(super) caller_owns_scope: bool,
    pub(super) flags: php_ir::module::ClassPropertyFlags,
    pub(super) default: Option<php_ir::ConstId>,
    pub(super) has_deferred_default: bool,
    pub(super) type_: Option<php_ir::IrReturnType>,
}

#[derive(Clone)]
pub(super) struct NativeInstancePropertyDeclaration {
    pub(super) owner_unit: Option<usize>,
    pub(super) owner: crate::compiled_unit::CompiledClass,
    pub(super) entry: php_ir::module::ClassPropertyEntry,
}

pub(super) fn native_instance_property_declaration(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
    property: &str,
    caller_function: u32,
) -> Option<NativeInstancePropertyDeclaration> {
    let mut candidate = normalize_class_name(class_name);
    if let Some(caller_name) =
        native_effective_calling_class(context, caller_function).map(|class| class.name.clone())
        && native_class_is_a(context, &candidate, &caller_name)
    {
        let scoped_owner = native_active_class_handle(context, &caller_name).map_or_else(
            || {
                native_external_class_handle(context, &caller_name)
                    .map(|(unit, class)| (Some(unit), class))
            },
            |class| Some((None, class)),
        );
        if let Some((owner_unit, owner)) = scoped_owner
            && let Some(entry) = owner
                .properties
                .iter()
                .find(|entry| {
                    !entry.flags.is_static && entry.flags.is_private && entry.name == property
                })
                .cloned()
        {
            return Some(NativeInstancePropertyDeclaration {
                owner_unit,
                owner,
                entry,
            });
        }
    }
    let mut visited = std::collections::BTreeSet::new();
    while visited.insert(candidate.clone()) {
        let (owner_unit, owner) = native_active_class_handle(context, &candidate).map_or_else(
            || {
                native_external_class_handle(context, &candidate)
                    .map(|(unit, class)| (Some(unit), class))
            },
            |class| Some((None, class)),
        )?;
        if let Some(entry) = owner
            .properties
            .iter()
            .find(|entry| !entry.flags.is_static && entry.name == property)
            .cloned()
        {
            return Some(NativeInstancePropertyDeclaration {
                owner_unit,
                owner,
                entry,
            });
        }
        candidate = normalize_class_name(owner.parent.as_ref()?);
    }
    None
}

pub(super) fn native_instance_property_readable(
    context: &NativeRequestColdState<'_>,
    declaration: &NativeInstancePropertyDeclaration,
    caller_function: u32,
) -> bool {
    if !declaration.entry.flags.is_private && !declaration.entry.flags.is_protected {
        return true;
    }
    let Some(caller) = native_effective_calling_class(context, caller_function) else {
        return false;
    };
    if declaration.entry.flags.is_private {
        caller.name == declaration.owner.name
    } else {
        native_class_is_a(context, &caller.name, &declaration.owner.name)
    }
}

pub(super) fn native_instance_property_writable(
    context: &NativeRequestColdState<'_>,
    declaration: &NativeInstancePropertyDeclaration,
    caller_function: u32,
) -> bool {
    let private = declaration.entry.flags.is_private || declaration.entry.flags.set_is_private;
    let protected =
        declaration.entry.flags.is_protected || declaration.entry.flags.set_is_protected;
    if !private && !protected {
        return true;
    }
    let Some(caller) = native_effective_calling_class(context, caller_function) else {
        return false;
    };
    if private {
        caller.name == declaration.owner.name
    } else {
        native_class_is_a(context, &caller.name, &declaration.owner.name)
    }
}

pub(super) fn native_static_property_declaration(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
    property: &str,
    caller_function: u32,
) -> Option<NativeStaticPropertyDeclaration> {
    let mut candidate = normalize_class_name(class_name);
    let mut visited = std::collections::BTreeSet::new();
    while visited.insert(candidate.clone()) {
        let (unit, class) = if let Some(class) = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == candidate)
        {
            (None, class)
        } else {
            let (unit, class) = native_external_class_ref(context, &candidate)?;
            (Some(unit), class)
        };
        if let Some(entry) = class
            .properties
            .iter()
            .find(|entry| entry.flags.is_static && entry.name == property)
        {
            return Some(NativeStaticPropertyDeclaration {
                owner_unit: unit,
                owner_name: class.name.clone(),
                owner_display_name: class.display_name.clone(),
                caller_owns_scope: class
                    .methods
                    .iter()
                    .any(|method| method.function.raw() == caller_function),
                flags: entry.flags,
                default: entry.default,
                has_deferred_default: entry.default_class_constant.is_some()
                    || entry.default_named_constant.is_some()
                    || entry.default_expr.is_some(),
                type_: entry.type_.clone(),
            });
        }
        candidate = normalize_class_name(class.parent.as_ref()?);
    }
    None
}

pub(super) fn native_external_method(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
    method: &str,
) -> Option<(NativeDynamicFunction, php_ir::module::ClassMethodEntry)> {
    let (mut unit, mut class) =
        native_external_class_handle(context, class_name).or_else(|| {
            let local = context
                .unit
                .classes
                .iter()
                .find(|class| class.name == normalize_class_name(class_name))?;
            native_external_class_handle(context, local.parent.as_deref()?)
        })?;
    loop {
        if let Some(entry) = class
            .methods
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(method))
            .cloned()
        {
            return Some((
                NativeDynamicFunction {
                    unit,
                    function: entry.function,
                },
                entry,
            ));
        }
        let parent = class.parent.as_deref()?;
        let normalized_parent = normalize_class_name(parent);
        let (parent_unit, parent_class) = context
            .current_dynamic_unit
            .and_then(|unit| {
                context
                    .dynamic_units
                    .get(unit)?
                    .compiled
                    .lookup_unit_class_handle(&normalized_parent)
                    .map(|class| (unit, class))
            })
            .or_else(|| native_external_class_handle(context, parent))?;
        unit = parent_unit;
        class = parent_class;
    }
}

pub(super) fn native_function_has_implicit_closure_this(function: &php_ir::IrFunction) -> bool {
    function.implicit_closure_this_local().is_some()
}

#[cfg(test)]
pub(super) fn native_backtrace_frame(
    compiled: &crate::compiled_unit::CompiledUnit,
    function: php_ir::FunctionId,
    called_class: Option<Arc<str>>,
    object: Option<php_runtime::api::ObjectRef>,
    arguments: request_state::NativeTraceArguments,
) -> NativeBacktraceFrame {
    let metadata = NativeFunctionMetadataPtr::from_compiled(compiled, function);
    native_backtrace_frame_from_metadata(metadata, called_class, object, arguments)
}

pub(super) fn native_backtrace_frame_from_metadata(
    metadata: Option<NativeFunctionMetadataPtr>,
    called_class: Option<Arc<str>>,
    object: Option<php_runtime::api::ObjectRef>,
    arguments: request_state::NativeTraceArguments,
) -> NativeBacktraceFrame {
    let fixed_argument_count = metadata.as_ref().map_or(0, |metadata| {
        metadata
            .params
            .iter()
            .position(|parameter| parameter.variadic)
            .unwrap_or(metadata.params.len())
            .min(arguments.len()) as u32
    });
    let class = metadata.as_ref().and_then(|metadata| {
        metadata
            .trace_class
            .as_ref()
            .map(|class| called_class.unwrap_or_else(|| Arc::clone(class)))
    });
    NativeBacktraceFrame {
        metadata,
        class,
        object,
        arguments,
        fixed_argument_count,
    }
}

pub(in crate::vm) fn resume_native_optimizing_exit(
    context: &mut NativeRequestColdState<'_>,
    active_artifact: php_jit::JitFunctionHandle,
    outcome: Result<php_jit::JitI64InvokeOutcome, php_jit::JitInvokeError>,
) -> Result<php_jit::JitI64InvokeOutcome, php_jit::JitInvokeError> {
    resume_native_optimizing_exit_with_artifact(context, Some(active_artifact), outcome)
        .map(|(_, outcome)| outcome)
}

pub(super) fn native_transition_metadata<'a>(
    handle: &'a php_jit::JitFunctionHandle,
    state: &php_jit::JitDeoptState,
) -> Option<&'a php_jit::JitNativeTransitionMetadata> {
    handle.region_state_metadata().and_then(|metadata| {
        metadata.native_transitions.iter().find(|entry| {
            entry.function.raw() == state.function_id
                && entry.continuation_id == state.continuation_id
        })
    })
}

pub(super) fn active_artifact_owns_published_transition(
    context: &NativeRequestColdState<'_>,
    handle: &php_jit::JitFunctionHandle,
    state: &php_jit::JitDeoptState,
) -> bool {
    let Some(metadata) = handle.region_state_metadata() else {
        return false;
    };
    if metadata.native_version != state.native_version
        || native_transition_metadata(handle, state).is_none()
    {
        return false;
    }
    let Some(function_entry) = metadata
        .function_entries
        .iter()
        .find(|entry| entry.function.raw() == state.function_id)
    else {
        return false;
    };
    context
        .compiled
        .prepared_deployment_image()
        .preferred_function_entries
        .get(state.function_id as usize)
        .is_some_and(|entry| {
            entry.load(std::sync::atomic::Ordering::Acquire) == function_entry.address
        })
}

pub(super) fn native_transition_owner_adjustments(
    source: &php_jit::JitNativeTransitionMetadata,
    target: &php_jit::JitNativeTransitionMetadata,
    state: &php_jit::JitDeoptState,
) -> (Vec<i64>, Vec<i64>) {
    let source_locals = source
        .owned_locals
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let target_locals = target
        .owned_locals
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let source_registers = source
        .owned_registers
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let target_registers = target
        .owned_registers
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let mut retain = Vec::new();
    let mut release = Vec::new();
    for local in source_locals.union(&target_locals).copied() {
        if !state.local_initialized(local) {
            continue;
        }
        let value = state.slots[local.index()];
        match (
            source_locals.contains(&local),
            target_locals.contains(&local),
        ) {
            (false, true) => retain.push(value),
            (true, false) => release.push(value),
            _ => {}
        }
    }
    for snapshot in 0..php_jit::JIT_DEOPT_MAX_REGISTERS {
        let initialized = state.initialized_register_mask
            & 1_u64
                .checked_shl(u32::try_from(snapshot).unwrap_or(u32::MAX))
                .unwrap_or(0)
            != 0;
        if !initialized {
            continue;
        }
        let register = php_ir::RegId::new(state.register_ids[snapshot]);
        let value = state.registers[snapshot];
        match (
            source_registers.contains(&register),
            target_registers.contains(&register),
        ) {
            (false, true) => retain.push(value),
            (true, false) => release.push(value),
            _ => {}
        }
    }
    (retain, release)
}

pub(super) fn reconcile_native_transition_owners(
    context: &mut NativeRequestColdState<'_>,
    source: &php_jit::JitFunctionHandle,
    target: &php_jit::JitFunctionHandle,
    state: &php_jit::JitDeoptState,
) -> Result<(), String> {
    let source = native_transition_metadata(source, state).ok_or_else(|| {
        format!(
            "optimizing transition {}:{} has no source ownership metadata",
            state.function_id, state.continuation_id
        )
    })?;
    let target = native_transition_metadata(target, state).ok_or_else(|| {
        format!(
            "optimizing transition {}:{} has no baseline ownership metadata",
            state.function_id, state.continuation_id
        )
    })?;
    let (retain, release) = native_transition_owner_adjustments(source, target, state);
    // Acquire the baseline-only owners first. If the same encoded value moves
    // between two ownership identities, it can never transiently reach zero.
    for value in retain {
        context.retain(value)?;
    }
    for value in release {
        context.release(value)?;
    }
    Ok(())
}

pub(super) fn remap_native_transition_registers(
    target: &php_jit::JitNativeTransitionMetadata,
    state: &php_jit::JitDeoptState,
) -> Result<php_jit::JitDeoptState, String> {
    let mut remapped = *state;
    remapped.initialized_register_mask = 0;
    remapped.register_ids.fill(0);
    remapped.registers.fill(0);
    for (target_slot, register) in target.live_registers.iter().copied().enumerate() {
        let Some(source_slot) = (0..php_jit::JIT_DEOPT_MAX_REGISTERS).find(|source_slot| {
            state.initialized_register_mask & (1_u64 << source_slot) != 0
                && state.register_ids[*source_slot] == register.raw()
        }) else {
            return Err(format!(
                "optimizing transition {}:{} did not publish live baseline register {}",
                state.function_id,
                state.continuation_id,
                register.raw()
            ));
        };
        remapped.register_ids[target_slot] = register.raw();
        remapped.registers[target_slot] = state.registers[source_slot];
        remapped.initialized_register_mask |= 1_u64 << target_slot;
    }
    Ok(remapped)
}

pub(super) fn resume_native_optimizing_exit_with_artifact(
    context: &mut NativeRequestColdState<'_>,
    mut active_artifact: Option<php_jit::JitFunctionHandle>,
    mut outcome: Result<php_jit::JitI64InvokeOutcome, php_jit::JitInvokeError>,
) -> Result<
    (
        Option<php_jit::JitFunctionHandle>,
        php_jit::JitI64InvokeOutcome,
    ),
    php_jit::JitInvokeError,
> {
    loop {
        let Ok(php_jit::JitI64InvokeOutcome::SideExit { status, state, .. }) = &outcome else {
            return outcome.map(|outcome| (active_artifact, outcome));
        };
        if *status != php_jit::JitCallStatus::RECOMPILE_REQUESTED.0 as i32 {
            return outcome.map(|outcome| (active_artifact, outcome));
        }
        let transition_instruction =
            context.instruction_for_continuation(state.function_id, state.continuation_id);
        let mut transition_reason = transition_instruction
            .as_ref()
            .map(|instruction| native_optimizing_transition_reason(&instruction.kind))
            .unwrap_or_else(|| std::borrow::Cow::Borrowed("optimizer_unknown"));
        if transition_reason.as_ref() == "optimizer_array:IssetDim" {
            let mut detail = match state.control_reserved {
                php_jit::JIT_OPTIMIZING_EXIT_ARRAY_NOT_TAGGED => "not_tagged",
                php_jit::JIT_OPTIMIZING_EXIT_ARRAY_VIEW_MISSING => "view_missing",
                php_jit::JIT_OPTIMIZING_EXIT_ARRAY_KEY_UNSUPPORTED => "key_unsupported",
                _ => "unknown",
            }
            .to_owned();
            if state.control_reserved == php_jit::JIT_OPTIMIZING_EXIT_ARRAY_NOT_TAGGED
                && let Some(instruction) = transition_instruction.as_ref()
                && let php_ir::InstructionKind::IssetDim { local, .. } = &instruction.kind
                && state.local_initialized(*local)
            {
                detail.push(':');
                detail.push_str(native_transition_value_kind(state.slots[local.index()]));
            }
            transition_reason =
                std::borrow::Cow::Owned(format!("{}:{detail}", transition_reason.as_ref()));
        } else if transition_reason.as_ref() == "optimizer_local:LoadLocal"
            && let Some(instruction) = transition_instruction.as_ref()
            && let php_ir::InstructionKind::LoadLocal { local, .. } = &instruction.kind
            && state.local_initialized(*local)
        {
            let stored = native_transition_direct_value_kind(context, state.slots[local.index()]);
            let next = context
                .instruction_for_continuation(
                    state.function_id,
                    state.continuation_id.saturating_add(1),
                )
                .map(|instruction| {
                    let rendered = format!("{:?}", instruction.kind);
                    rendered
                        .split_once([' ', '{', '('])
                        .map_or(rendered.as_str(), |(name, _)| name)
                        .to_owned()
                })
                .unwrap_or_else(|| "terminal".to_owned());
            transition_reason = std::borrow::Cow::Owned(format!(
                "{}:{stored}:next_{next}",
                transition_reason.as_ref()
            ));
        } else if transition_reason.as_ref() == "optimizer_array:AssignDim"
            && let Some(instruction) = transition_instruction.as_ref()
            && let php_ir::InstructionKind::AssignDim { local, .. } = &instruction.kind
            && state.local_initialized(*local)
        {
            let encoded = state.slots[local.index()];
            let raw = native_transition_value_kind(encoded);
            let stored = native_transition_direct_value_kind(context, encoded);
            let descriptor = php_jit::jit_decode_runtime_value(encoded).map_or_else(
                || "immediate".to_owned(),
                |index| {
                    if index >= php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE {
                        return context
                            .direct_value_slots
                            .get((index - php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE) as usize)
                            .map_or_else(
                                || "direct_missing".to_owned(),
                                |slot| format!("direct_kind_{}_refs_{}", slot.kind, slot.refcount),
                            );
                    }
                    "cold_record".to_owned()
                },
            );
            transition_reason = std::borrow::Cow::Owned(format!(
                "{}:{raw}:{stored}:{descriptor}",
                transition_reason.as_ref()
            ));
        } else if transition_reason
            .as_ref()
            .starts_with("optimizer_call:CallFunction:")
            && let Some(instruction) = transition_instruction.as_ref()
            && let php_ir::InstructionKind::CallFunction { args, .. } = &instruction.kind
        {
            let values = args
                .iter()
                .take(4)
                .map(|argument| {
                    let encoded = match argument.value {
                        php_ir::Operand::Local(local) if state.local_initialized(local) => {
                            Some(state.slots[local.index()])
                        }
                        php_ir::Operand::Register(register) => (0
                            ..php_jit::JIT_DEOPT_MAX_REGISTERS)
                            .find(|index| {
                                state.initialized_register_mask & (1_u64 << index) != 0
                                    && state.register_ids[*index] == register.raw()
                            })
                            .map(|index| state.registers[index]),
                        php_ir::Operand::Constant(_) | php_ir::Operand::Local(_) => None,
                    };
                    encoded.map_or_else(
                        || "constant_or_unpublished".to_owned(),
                        |encoded| {
                            format!(
                                "{}/{}",
                                native_transition_value_kind(encoded),
                                native_transition_direct_value_kind(context, encoded),
                            )
                        },
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            transition_reason = std::borrow::Cow::Owned(format!(
                "{}:values_{values}:detail_{:#x}",
                transition_reason.as_ref(),
                state.control_reserved,
            ));
        }
        let transition_started = context
            .options
            .collect_counters
            .then(std::time::Instant::now);
        let function = php_ir::FunctionId::new(state.function_id);
        let replays_store = transition_instruction.as_ref().is_some_and(|instruction| {
            matches!(instruction.kind, php_ir::InstructionKind::StoreLocal { .. })
        });
        let fallback = NativeExecutionTarget {
            unit: context.current_dynamic_unit,
            function,
            called_class: context.called_classes.last().cloned(),
            scope_class: context
                .lexical_scope_classes
                .last()
                .map(|scope| Arc::from(scope.as_str())),
        };
        let target = context
            .native_execution_target_from_state(state, Some(&fallback))
            .map_err(|_| php_jit::JitInvokeError::MissingNativeTransition {
                function: state.function_id,
                continuation: state.continuation_id,
            })?;
        let state = *state;
        let carried_artifact = active_artifact.clone();
        let (baseline, resumed) = context
            .run_in_native_execution_target(&target, |context| -> Result<_, String> {
                // A direct linked callee can side-exit without returning
                // through the caller's Rust coordinator. The carried handle
                // may therefore own the caller graph, and dense unit-local
                // FunctionIds can make unrelated metadata appear to match.
                // Match its exact function entry against the runtime-view-
                // selected unit's preferred publication cell before using
                // it. This preserves the generation that actually produced
                // the exit without trusting a coincidental dense ID.
                let source = if let Some(source) = carried_artifact.as_ref().filter(|source| {
                    active_artifact_owns_published_transition(context, source, &state)
                }) {
                    source.clone()
                } else {
                    ensure_native_entry(context, function)?
                };
                let source_metadata =
                    native_transition_metadata(&source, &state).ok_or_else(|| {
                        format!(
                            "optimizing transition {}:{} has no active-unit source metadata",
                            state.function_id, state.continuation_id
                        )
                    })?;
                if source_metadata.native_version != state.native_version {
                    let error = format!(
                        "optimizing transition {}:{} source tier {} does not match state tier {}",
                        state.function_id,
                        state.continuation_id,
                        source_metadata.native_version,
                        state.native_version
                    );
                    cold_diagnostics::record_native_helper_failure(context, error.clone());
                    return Err(error);
                }
                let baseline = ensure_native_generic_entry(context, function)?;
                if let Err(error) =
                    reconcile_native_transition_owners(context, &source, &baseline, &state)
                {
                    cold_diagnostics::record_native_helper_failure(context, error.clone());
                    return Err(error);
                }
                let Some(target_metadata) = native_transition_metadata(&baseline, &state) else {
                    let error = format!(
                        "optimizing transition {}:{} has no reconciled baseline metadata",
                        state.function_id, state.continuation_id
                    );
                    cold_diagnostics::record_native_helper_failure(context, error.clone());
                    return Err(error);
                };
                let baseline_state =
                    match remap_native_transition_registers(target_metadata, &state) {
                        Ok(state) => state,
                        Err(error) => {
                            cold_diagnostics::record_native_helper_failure(context, error.clone());
                            return Err(error);
                        }
                    };
                let runtime = context.native_runtime_ptr();
                context.baseline_transition_store_owner_pending = replays_store;
                let resumed = baseline.invoke_i64_native_transition_with_unwind_runtime(
                    &baseline_state,
                    php_jit::JIT_RUNTIME_ABI_HASH,
                    runtime,
                    |types, value| native_catch_matches(context, types, value),
                );
                context.baseline_transition_store_owner_pending = false;
                Ok((baseline, resumed))
            })
            .map_err(|_| php_jit::JitInvokeError::MissingNativeTransition {
                function: state.function_id,
                continuation: state.continuation_id,
            })?;
        outcome = resumed;
        active_artifact = Some(baseline);
        if let Some(started) = transition_started {
            context.record_native_transition(transition_reason.as_ref(), started.elapsed(), 0);
        }
    }
}

pub(super) fn native_transition_value_kind(encoded: i64) -> &'static str {
    let encoded = encoded as u64;
    match encoded & php_jit::JIT_VALUE_RUNTIME_KIND_MASK {
        php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG => "reference",
        php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG => "array",
        php_jit::JIT_VALUE_RUNTIME_OBJECT_TAG => "object",
        php_jit::JIT_VALUE_RUNTIME_STRING_TAG => "string",
        php_jit::JIT_VALUE_RUNTIME_FLOAT_TAG => "float",
        php_jit::JIT_VALUE_RUNTIME_RESOURCE_TAG => "resource",
        php_jit::JIT_VALUE_RUNTIME_CALLABLE_TAG => "callable",
        php_jit::JIT_VALUE_RUNTIME_GENERATOR_TAG => "generator",
        php_jit::JIT_VALUE_RUNTIME_FIBER_TAG => "fiber",
        php_jit::JIT_VALUE_RUNTIME_ITERATOR_TAG => "iterator",
        _ if encoded == php_jit::jit_encode_constant(u32::MAX) as u64 => "null",
        _ if encoded & php_jit::JIT_VALUE_TAG_MASK == php_jit::JIT_VALUE_CONSTANT_TAG => "constant",
        _ => "immediate",
    }
}

pub(super) fn native_transition_direct_value_kind(
    context: &NativeRequestColdState<'_>,
    encoded: i64,
) -> &'static str {
    if let Some(index) = NativeRequestColdState::direct_value_index(encoded)
        && let Some(slot) = context
            .direct_value_slots
            .get(index)
            .filter(|slot| slot.refcount != 0)
    {
        return match slot.kind {
            php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE => "prepared_callable",
            php_jit::JIT_NATIVE_VALUE_VIEW_COLD_GENERATOR => "materialized_generator",
            php_jit::JIT_NATIVE_VALUE_VIEW_FOREACH_DIRECT
            | php_jit::JIT_NATIVE_VALUE_VIEW_COLD_ITERATOR => "retired_cold_iterator",
            _ => native_transition_value_kind(encoded),
        };
    }
    let Some(_) = php_jit::jit_decode_runtime_value(encoded) else {
        return native_transition_value_kind(encoded);
    };
    "missing"
}

pub(super) fn native_optimizing_transition_reason(
    kind: &php_ir::InstructionKind,
) -> std::borrow::Cow<'static, str> {
    use php_ir::InstructionKind;

    let family = match kind {
        InstructionKind::LoadLocal { .. }
        | InstructionKind::StoreLocal { .. }
        | InstructionKind::Discard { .. }
        | InstructionKind::IssetLocal { .. }
        | InstructionKind::EmptyLocal { .. }
        | InstructionKind::UnsetLocal { .. } => "optimizer_local",
        InstructionKind::Unary { .. }
        | InstructionKind::Binary { .. }
        | InstructionKind::Compare { .. }
        | InstructionKind::Cast { .. } => "optimizer_scalar",
        InstructionKind::NewArray { .. }
        | InstructionKind::ArrayInsert { .. }
        | InstructionKind::ArraySpread { .. }
        | InstructionKind::FetchDim { .. }
        | InstructionKind::AssignDim { .. }
        | InstructionKind::AppendDim { .. }
        | InstructionKind::UnsetDim { .. }
        | InstructionKind::IssetDim { .. }
        | InstructionKind::EmptyDim { .. } => "optimizer_array",
        InstructionKind::ForeachInit { .. }
        | InstructionKind::ForeachInitRef { .. }
        | InstructionKind::ForeachNext { .. }
        | InstructionKind::ForeachNextRef { .. }
        | InstructionKind::ForeachCleanup { .. } => "optimizer_foreach",
        InstructionKind::FetchProperty { .. }
        | InstructionKind::AssignProperty { .. }
        | InstructionKind::FetchDynamicStaticProperty { .. }
        | InstructionKind::AssignDynamicStaticProperty { .. }
        | InstructionKind::FetchObjectClassName { .. } => "optimizer_property",
        InstructionKind::BindReference { .. }
        | InstructionKind::BindReferenceDim { .. }
        | InstructionKind::BindReferenceProperty { .. }
        | InstructionKind::BindReferenceFromProperty { .. }
        | InstructionKind::BindReferenceFromPropertyDim { .. }
        | InstructionKind::BindReferencePropertyDim { .. }
        | InstructionKind::BindReferenceDimFromProperty { .. }
        | InstructionKind::BindReferenceFromDim { .. }
        | InstructionKind::BindReferenceFromStaticPropertyDim { .. }
        | InstructionKind::BindReferenceStaticProperty { .. }
        | InstructionKind::BindReferenceFromCall { .. }
        | InstructionKind::BindReferenceFromMethodCall { .. } => "optimizer_reference",
        InstructionKind::CallFunction { .. }
        | InstructionKind::CallMethod { .. }
        | InstructionKind::CallStaticMethod { .. }
        | InstructionKind::CallClosure { .. }
        | InstructionKind::CallCallable { .. }
        | InstructionKind::Pipe { .. }
        | InstructionKind::NewObject { .. }
        | InstructionKind::DynamicNewObject { .. } => "optimizer_call",
        InstructionKind::Include { .. }
        | InstructionKind::Eval { .. }
        | InstructionKind::DeclareFunction { .. }
        | InstructionKind::DeclareClass { .. } => "optimizer_dynamic_code",
        _ => "optimizer_other",
    };
    // This runs only while diagnostic counters are enabled. Preserve the
    // exact IR opcode, but not its operands, so an aggregate family cannot
    // hide the next dominant warm transition after an earlier exit is
    // removed.
    if let InstructionKind::Binary { op, .. } = kind {
        return format!("{family}:Binary:{op:?}").into();
    }
    if let InstructionKind::CallFunction { name, args, .. } = kind {
        let named = args
            .iter()
            .filter(|argument| argument.name.is_some())
            .count();
        let unpacked = args.iter().filter(|argument| argument.unpack).count();
        return format!(
            "{family}:CallFunction:{}:argc{}:named{}:unpack{}",
            name.trim_start_matches('\\').to_ascii_lowercase(),
            args.len(),
            named,
            unpacked,
        )
        .into();
    }
    let debug = format!("{kind:?}");
    let end = debug.find([' ', '{', '(']).unwrap_or(debug.len());
    format!("{family}:{}", &debug[..end]).into()
}

pub(super) fn native_class_is_a(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
    target: &str,
) -> bool {
    let target = normalize_class_name(target);
    let class_name = normalize_class_name(class_name);
    if class_name == "arrayiterator" && matches!(target.as_str(), "iterator" | "traversable") {
        return true;
    }
    let cache_key = (context.external_signature_epoch, class_name.clone());
    if let Some(ancestry) = context
        .runtime_class_ancestry_cache
        .borrow()
        .get(&cache_key)
    {
        return ancestry.contains(&target);
    }
    let mut pending = vec![class_name];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(candidate) = pending.pop() {
        if !visited.insert(candidate.clone()) {
            continue;
        }
        if let Some(class) = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == candidate)
        {
            if let Some(parent) = &class.parent {
                pending.push(normalize_class_name(parent));
            }
            pending.extend(
                class
                    .interfaces
                    .iter()
                    .map(|interface| normalize_class_name(interface)),
            );
        } else if let Some((_, class)) = native_external_class_ref(context, &candidate) {
            if let Some(parent) = &class.parent {
                pending.push(normalize_class_name(parent));
            }
            pending.extend(
                class
                    .interfaces
                    .iter()
                    .map(|interface| normalize_class_name(interface)),
            );
        } else if let Some(class) =
            php_std::ExtensionRegistry::standard_library().enabled_class(&candidate)
            && let Some(metadata) = class.source_metadata()
        {
            if let Some(parent) = metadata.parent {
                pending.push(normalize_class_name(parent));
            }
            pending.extend(
                metadata
                    .interfaces
                    .iter()
                    .map(|interface| normalize_class_name(interface)),
            );
        }
    }
    let result = visited.contains(&target);
    context
        .runtime_class_ancestry_cache
        .borrow_mut()
        .insert(cache_key, visited);
    result
}

pub(super) fn native_method_in_hierarchy(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
    method: &str,
) -> Option<php_ir::FunctionId> {
    let mut candidate = normalize_class_name(class_name);
    loop {
        let class = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == candidate)?;
        if let Some(entry) = class
            .methods
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(method))
        {
            return Some(entry.function);
        }
        candidate = normalize_class_name(class.parent.as_ref()?);
    }
}

pub(super) fn native_function_is_generator(
    context: &NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
) -> bool {
    context
        .unit
        .functions
        .get(function.index())
        .is_some_and(|function| {
            function.flags.is_generator
                || function
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .any(|instruction| {
                        matches!(
                            instruction.kind,
                            php_ir::InstructionKind::Yield { .. }
                                | php_ir::InstructionKind::YieldFrom { .. }
                        )
                    })
        })
}

pub(super) fn native_function_requires_non_reference_trampoline(
    function: &php_ir::IrFunction,
    method_scope_sensitive: bool,
) -> bool {
    function.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            matches!(
                instruction.kind,
                php_ir::InstructionKind::Yield { .. } | php_ir::InstructionKind::YieldFrom { .. }
            ) || method_scope_sensitive
                && matches!(
                    &instruction.kind,
                    php_ir::InstructionKind::FetchClassConstant {
                        class_name,
                        ..
                    } | php_ir::InstructionKind::CallStaticMethod {
                        class_name,
                        ..
                    } if class_name.eq_ignore_ascii_case("static")
                )
        })
    }) || function.attributes.iter().any(|attribute| {
        attribute
            .resolved_name
            .as_deref()
            .or(attribute.fallback_name.as_deref())
            .unwrap_or(&attribute.name)
            .trim_start_matches('\\')
            .eq_ignore_ascii_case("deprecated")
    })
}

pub(super) fn native_function_exception_routes(
    function: php_ir::FunctionId,
    definition: &php_ir::IrFunction,
) -> Option<php_ir::FunctionId> {
    definition
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .any(|instruction| {
            matches!(
                instruction.kind,
                php_ir::InstructionKind::EnterTry { catch: Some(_), .. }
                    | php_ir::InstructionKind::EnterTry {
                        finally: Some(_),
                        ..
                    }
            )
        })
        .then_some(function)
}

pub(super) fn native_calling_class<'a>(
    context: &'a NativeRequestColdState<'_>,
    function: u32,
) -> Option<&'a php_ir::ClassEntry> {
    context.unit.classes.iter().find(|class| {
        class
            .methods
            .iter()
            .any(|method| method.function.raw() == function)
    })
}

pub(super) fn native_effective_calling_class<'a>(
    context: &'a NativeRequestColdState<'_>,
    function: u32,
) -> Option<&'a php_ir::ClassEntry> {
    native_calling_class(context, function).or_else(|| {
        let scope = context.lexical_scope_classes.last()?;
        let normalized = normalize_class_name(scope);
        context
            .unit
            .classes
            .iter()
            .find(|class| class.name == normalized)
    })
}

pub(super) fn native_resolve_scoped_class_name(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
    caller_function: u32,
) -> Result<String, String> {
    match class_name.to_ascii_lowercase().as_str() {
        "self" => native_effective_calling_class(context, caller_function)
            .map(|class| class.display_name.clone())
            .ok_or_else(|| "Cannot use \"self\" in the global scope".to_owned()),
        "static" => context
            .called_classes
            .last()
            .map(|class| class.to_string())
            .or_else(|| {
                native_effective_calling_class(context, caller_function)
                    .map(|class| class.display_name.clone())
            })
            .ok_or_else(|| "Cannot use \"static\" in the global scope".to_owned()),
        "parent" => native_effective_calling_class(context, caller_function)
            .and_then(|class| {
                class
                    .parent_display_name
                    .clone()
                    .or_else(|| class.parent.clone())
            })
            .ok_or_else(|| "Cannot use \"parent\" when no parent scope is active".to_owned()),
        _ => Ok(class_name.to_owned()),
    }
}

pub(super) fn native_method_access_error(
    context: &NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
    caller_function: u32,
    _late_static_call: bool,
) -> Option<String> {
    let (declaring_class, method) = context.unit.classes.iter().find_map(|class| {
        class
            .methods
            .iter()
            .find(|method| method.function == function)
            .map(|method| (class, method))
    })?;
    if !method.flags.is_private && !method.flags.is_protected {
        return None;
    }
    let caller = native_effective_calling_class(context, caller_function);
    if method.flags.is_private && caller.is_none_or(|caller| caller.name != declaring_class.name) {
        if caller.is_none() {
            return Some(format!(
                "Call to private method {}::{}() from global scope",
                declaring_class.display_name, method.name
            ));
        }
        return Some(format!(
            "Cannot access private method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    if method.flags.is_protected
        && caller
            .is_none_or(|caller| !native_class_is_a(context, &caller.name, &declaring_class.name))
    {
        return Some(format!(
            "Cannot access protected method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    None
}

pub(super) fn native_external_method_access_error(
    context: &NativeRequestColdState<'_>,
    target: NativeDynamicFunction,
    caller_function: u32,
    _late_static_call: bool,
) -> Option<String> {
    let unit = context.dynamic_units.get(target.unit)?.compiled.unit();
    let (declaring_class, method) = unit.classes.iter().find_map(|class| {
        class
            .methods
            .iter()
            .find(|method| method.function == target.function)
            .map(|method| (class, method))
    })?;
    if !method.flags.is_private && !method.flags.is_protected {
        return None;
    }
    let caller = native_effective_calling_class(context, caller_function);
    if method.flags.is_private && caller.is_none_or(|caller| caller.name != declaring_class.name) {
        if caller.is_none() {
            return Some(format!(
                "Call to private method {}::{}() from global scope",
                declaring_class.display_name, method.name
            ));
        }
        return Some(format!(
            "Cannot access private method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    if method.flags.is_protected
        && caller
            .is_none_or(|caller| !native_class_is_a(context, &caller.name, &declaring_class.name))
    {
        return Some(format!(
            "Cannot access protected method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    None
}
