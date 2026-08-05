//! Native-only capability behavior. Cold pointer publication lives in cold_publication.

use super::*;
use php_ir::module::{normalize_class_name, normalized_class_name};

fn native_exact_function_requires_non_reference_trampoline(
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
                    php_ir::InstructionKind::CallStaticMethod {
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

fn native_exact_php_function_exists(name: &str) -> bool {
    if matches!(
        name,
        "print"
            | "mhash"
            | "mhash_count"
            | "mhash_get_block_size"
            | "mhash_get_hash_name"
            | "mhash_keygen_s2k"
    ) {
        return false;
    }
    php_std::introspection::function_exists(php_std::ExtensionRegistry::standard_library(), name)
        || php_extensions::BuiltinRegistry::new().contains(name)
}

fn native_exact_internal_class_constant_exists(name: &str) -> bool {
    let Some((class_name, constant_name)) = name.rsplit_once("::") else {
        return false;
    };
    php_std::ExtensionRegistry::standard_library()
        .enabled_class(class_name)
        .is_some()
        && php_std::generated::arginfo::constant_metadata_in_hierarchy(class_name, constant_name)
            .is_some()
}

impl NativeSymbolQueryCapability {
    #[allow(unsafe_code)] // Safety: published class-plan metadata is immutable for the request.
    pub(crate) fn active_compiled(&self) -> Option<&crate::compiled_unit::CompiledUnit> {
        unsafe { self.active_compiled.as_ref() }
    }

    #[allow(unsafe_code)]
    pub(crate) fn current_dynamic_unit(&self) -> Option<usize> {
        unsafe { self.current_dynamic_unit.as_ref() }
            .copied()
            .flatten()
    }

    #[allow(unsafe_code)]
    pub(crate) fn dynamic_units(&self) -> Option<&[NativeDynamicUnit]> {
        unsafe { self.dynamic_units.as_ref() }.map(Vec::as_slice)
    }

    /// Selects the immutable compiled unit that owns one published runtime
    /// view. Generated cross-unit calls temporarily install this same view,
    /// so exact publication can resolve target-local numeric metadata without
    /// consulting a dynamic operation dispatcher.
    pub(crate) fn compiled_for_runtime_view(
        &self,
        view: &php_jit::JitNativeRuntimeView,
    ) -> Option<&crate::compiled_unit::CompiledUnit> {
        let active = self.active_compiled()?;
        if active
            .prepared_deployment_image()
            .generic_function_entries
            .as_ptr() as usize as u64
            == view.trusted_generic_function_entries
        {
            return Some(active);
        }
        self.dynamic_units()?
            .iter()
            .find(|unit| {
                unit.published_runtime_view.trusted_generic_function_entries
                    == view.trusted_generic_function_entries
            })
            .map(|unit| &unit.compiled)
    }

    #[allow(unsafe_code)]
    pub(crate) fn class_is_visible(&self, normalized: &str) -> bool {
        unsafe { self.deployment_classes.as_ref() }
            .is_some_and(|classes| classes.as_ref().contains(normalized))
            || unsafe { self.dynamic_classes.as_ref() }
                .is_some_and(|classes| classes.contains(normalized))
    }

    #[allow(unsafe_code)]
    pub(crate) fn external_class_handle(
        &self,
        name: &str,
    ) -> Option<crate::compiled_unit::CompiledClass> {
        let requested = normalized_class_name(name);
        let normalized = unsafe { self.class_aliases.as_ref() }
            .and_then(|aliases| aliases.get(requested.as_ref()))
            .map_or(requested.as_ref(), String::as_str);
        // Deployment metadata describes classes that can become visible; it
        // is not a publication record.  Cross-unit native access is legal
        // only after include/autoload installed the owning unit mapping.
        let unit = unsafe { self.external_class_units.as_ref() }
            .and_then(|classes| classes.get(normalized).copied())?;
        if self.current_dynamic_unit() == Some(unit) {
            return None;
        }
        self.dynamic_units()?
            .get(unit)?
            .compiled
            .lookup_unit_class_handle(normalized)
    }

    pub(crate) fn class_handle(&self, name: &str) -> Option<crate::compiled_unit::CompiledClass> {
        let normalized = normalize_class_name(name);
        self.active_compiled()?
            .lookup_unit_class_handle(&normalized)
            .or_else(|| self.external_class_handle(&normalized))
    }

    /// Resolve one visible class name to its request-stable allocation plan.
    /// The returned address names only immutable published metadata; object
    /// allocation and constructor execution remain generated operations.
    #[allow(unsafe_code)]
    pub(crate) fn class_plan(
        &self,
        name: &str,
        active_view: &php_jit::JitNativeRuntimeView,
    ) -> Option<u64> {
        fn plan_at(view: &php_jit::JitNativeRuntimeView, index: usize) -> Option<u64> {
            if view.abi_version != php_jit::JIT_RUNTIME_ABI_VERSION
                || view.trusted_class_plans == 0
                || index >= view.trusted_class_plan_count as usize
            {
                return None;
            }
            let plans =
                view.trusted_class_plans as usize as *const php_jit::JitNativePreparedClassPlan;
            // SAFETY: publication owns a dense immutable plan array for the
            // lifetime of this request, and the index was checked above.
            #[allow(unsafe_code)] // Safety: the checked dense plan index is publication-stable.
            let plan = unsafe { &*plans.add(index) };
            (plan.state == php_jit::JIT_NATIVE_PREPARED_CLASS_ALLOCATABLE && plan.prepared != 0)
                .then(|| std::ptr::from_ref(plan) as usize as u64)
        }

        let requested = normalized_class_name(name);
        let normalized = unsafe { self.class_aliases.as_ref() }
            .and_then(|aliases| aliases.get(requested.as_ref()))
            .map_or(requested.as_ref(), String::as_str);
        if let Some(index) = self
            .active_compiled()?
            .unit()
            .classes
            .iter()
            .position(|class| normalized_class_name(&class.name).as_ref() == normalized)
            && let Some(plan) = plan_at(active_view, index)
        {
            return Some(plan);
        }
        let unit_index = unsafe { self.external_class_units.as_ref() }
            .and_then(|classes| classes.get(normalized).copied());
        let unit_index = unit_index?;
        let unit = self.dynamic_units()?.get(unit_index)?;
        let index = unit
            .compiled
            .unit()
            .classes
            .iter()
            .position(|class| normalized_class_name(&class.name).as_ref() == normalized)?;
        plan_at(&unit.published_runtime_view, index)
    }

    pub(crate) fn caller_class(&self, function: u32) -> Option<String> {
        self.active_compiled()?
            .unit()
            .classes
            .iter()
            .find(|class| {
                class
                    .methods
                    .iter()
                    .any(|method| method.function.raw() == function)
            })
            .map(|class| class.name.clone())
    }

    pub(crate) fn class_lineage_any(
        &self,
        name: &str,
        predicate: &mut impl FnMut(&crate::compiled_unit::CompiledClass) -> bool,
    ) -> bool {
        fn visit(
            symbols: &NativeSymbolQueryCapability,
            name: &str,
            depth: usize,
            predicate: &mut impl FnMut(&crate::compiled_unit::CompiledClass) -> bool,
        ) -> bool {
            if depth >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
                return false;
            }
            let Some(class) = symbols.class_handle(name) else {
                return false;
            };
            if predicate(&class) {
                return true;
            }
            class
                .parent
                .as_deref()
                .is_some_and(|parent| visit(symbols, parent, depth + 1, predicate))
        }
        visit(self, name, 0, predicate)
    }

    /// Resolves an exact class/interface ancestry query from the published
    /// unit, deployment, and internal-class metadata. `None` means some
    /// ancestry node is not represented by this capability and must take the
    /// instruction's single baseline continuation.
    #[allow(unsafe_code)] // Safety: ancestry metadata remains immutable for the request.
    pub(crate) fn class_is_a(&self, class_name: &str, target: &str) -> Option<bool> {
        fn visit(
            symbols: &NativeSymbolQueryCapability,
            candidate: &str,
            target: &str,
            depth: usize,
        ) -> Option<bool> {
            if depth >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
                return None;
            }
            let candidate = normalize_class_name(candidate);
            if candidate == target {
                return Some(true);
            }
            if candidate == "arrayiterator" && matches!(target, "iterator" | "traversable") {
                return Some(true);
            }
            if let Some(class) = symbols.class_handle(&candidate) {
                if let Some(parent) = class.parent.as_deref()
                    && visit(symbols, parent, target, depth + 1)?
                {
                    return Some(true);
                }
                for interface in &class.interfaces {
                    if visit(symbols, interface, target, depth + 1)? {
                        return Some(true);
                    }
                }
                return Some(false);
            }
            if let Some(class) =
                php_std::ExtensionRegistry::standard_library().enabled_class(&candidate)
                && let Some(metadata) = class.source_metadata()
            {
                if let Some(parent) = metadata.parent
                    && visit(symbols, parent, target, depth + 1)?
                {
                    return Some(true);
                }
                for interface in metadata.interfaces {
                    if visit(symbols, interface, target, depth + 1)? {
                        return Some(true);
                    }
                }
                return Some(false);
            }
            None
        }

        let target = normalize_class_name(target);
        let target = unsafe { self.class_aliases.as_ref() }
            .and_then(|aliases| aliases.get(&target))
            .map_or(target.as_str(), String::as_str)
            .to_owned();
        visit(self, class_name, &target, 0)
    }

    #[allow(unsafe_code)] // Safety: external function publication remains stable for the request.
    pub(crate) fn constant_exists(&self, name: &str) -> bool {
        unsafe { self.native_dynamic_constants.as_ref() }
            .is_some_and(|values| values.contains_key(name))
            || self.active_compiled().is_some_and(|compiled| {
                compiled
                    .unit()
                    .constant_table
                    .iter()
                    .any(|constant| constant.name == name)
            })
            || native_exact_internal_class_constant_exists(name)
            || php_std::ExtensionRegistry::standard_library()
                .enabled_constant(name)
                .and_then(php_std::ConstantDescriptor::value)
                .is_some()
    }

    #[allow(unsafe_code)] // Safety: external method publication remains stable for the request.
    pub(crate) fn native_constants(&self) -> Option<&std::collections::BTreeMap<String, i64>> {
        unsafe { self.native_dynamic_constants.as_ref() }
    }

    #[allow(unsafe_code)] // Safety: cross-unit method metadata remains stable for the request.
    pub(crate) fn dynamic_constant_sites(
        &self,
        name: &str,
        trusted_constant_slots: u64,
    ) -> (*const usize, usize) {
        // A generated include invokes its published entry after the cold
        // coordinator has restored the caller's active-unit fields. Resolve
        // the unit-scoped site table from the same stable slot arena carried
        // by the generated runtime view; consulting only the caller's map
        // leaves the included unit's FetchConst slots unpublished.
        let dynamic_sites = (trusted_constant_slots != 0)
            .then(|| unsafe { self.dynamic_units.as_ref() })
            .flatten()
            .and_then(|units| {
                units.iter().find(|unit| {
                    unit.published_runtime_view.trusted_constant_slots == trusted_constant_slots
                })
            })
            .and_then(|unit| unit.trusted_dynamic_constant_sites(name));
        let sites: &[usize] = dynamic_sites
            .or_else(|| {
                unsafe { self.trusted_dynamic_constant_sites.as_ref() }
                    .and_then(|sites| sites.get(name))
                    .map(Vec::as_slice)
            })
            .unwrap_or(&[]);
        (sites.as_ptr(), sites.len())
    }

    /// Returns every already-published unit group that observes one newly
    /// defined request constant. The active unit contributes its live slot
    /// table; inactive included units retain their own stable published views.
    /// Units not yet published need no mutation because activation prepares
    /// their constant slots from the authoritative request map.
    #[allow(unsafe_code)] // Safety: request-owned unit packages and views remain stable for the activation.
    pub(crate) fn dynamic_constant_site_groups(
        &self,
        name: &str,
        active_slots: u64,
    ) -> Vec<(u64, *const usize, usize)> {
        let (active_indices, active_count) = self.dynamic_constant_sites(name, active_slots);
        let mut groups = Vec::with_capacity(1);
        if active_slots != 0 && active_count != 0 {
            groups.push((active_slots, active_indices, active_count));
        }
        if let Some(units) = unsafe { self.dynamic_units.as_ref() } {
            for unit in units {
                let slots = unit.published_runtime_view.trusted_constant_slots;
                if slots == 0 || slots == active_slots {
                    continue;
                }
                let Some(indices) = unit.trusted_dynamic_constant_sites(name) else {
                    continue;
                };
                if !indices.is_empty() {
                    groups.push((slots, indices.as_ptr(), indices.len()));
                }
            }
        }
        groups
    }

    #[allow(unsafe_code)]
    pub(crate) fn function_exists(&self, name: &str) -> bool {
        let normalized = name.to_ascii_lowercase();
        let active = self.active_compiled().is_some_and(|compiled| {
            compiled
                .unit()
                .function_table
                .iter()
                .any(|entry| entry.name.eq_ignore_ascii_case(name))
        });
        let dynamic = unsafe { self.dynamic_functions.as_ref() }.is_some_and(|functions| {
            functions.contains_key(name) || functions.contains_key(&normalized)
        });
        let external = unsafe { self.external_functions.as_ref() }.is_some_and(|functions| {
            functions.contains_key(name) || functions.contains_key(&normalized)
        });
        let deployment = unsafe { self.deployment_functions.as_ref() }
            .is_some_and(|functions| functions.as_ref().contains_key(normalized.as_str()));
        let visible = unsafe { self.visible_function_names.as_ref() }
            .is_some_and(|functions| functions.contains(&normalized));
        active
            || dynamic
            || external
            || deployment
            || visible
            || native_exact_php_function_exists(&normalized)
    }

    pub(crate) fn same_unit_callable_plan(&self, name: &str) -> Option<NativeFixedCallablePlan> {
        let compiled = self.active_compiled()?;
        let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
        let function = compiled.lookup_function(&normalized).or_else(|| {
            normalized
                .rsplit_once('\\')
                .and_then(|(_, basename)| compiled.lookup_function(basename))
        })?;
        native_fixed_callable_plan(compiled, function, false)
    }

    /// Resolve a fixed function binding and publish the target entry-cell
    /// view once. Generated callers consume this record directly; Rust does
    /// not invoke the resolved PHP body.
    #[allow(unsafe_code)]
    pub(crate) fn callable_plan(&self, name: &str) -> Option<NativeFixedCallablePlan> {
        if let Some(plan) = self.same_unit_callable_plan(name) {
            return Some(plan);
        }
        let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
        let target = unsafe { self.external_functions.as_ref() }.and_then(|functions| {
            functions
                .get(name)
                .or_else(|| functions.get(&normalized))
                .copied()
        })?;
        let unit = self.dynamic_units()?.get(target.unit)?;
        let mut plan = native_fixed_callable_plan(&unit.compiled, target.function, false)?;
        plan.runtime_view = std::ptr::from_ref(&*unit.published_runtime_view) as usize as u64;
        Some(plan)
    }

    /// Resolve one public method against the immutable same-unit hierarchy.
    ///
    /// Callable publication is the semantic boundary: the exact method
    /// identity, staticness and fixed by-value signature are recorded once.
    /// Dynamic classes, inaccessible methods, magic dispatch, and unresolved
    /// late-static calls remain on the single baseline continuation. Direct
    /// late-static constant sites are published with the callable binding.
    pub(crate) fn same_unit_method_callable_plan(
        &self,
        class_name: &str,
        method_name: &str,
        object_target: bool,
    ) -> Option<NativeFixedCallablePlan> {
        let compiled = self.active_compiled()?;
        native_compiled_method_callable_plan(compiled, class_name, method_name, object_target)
    }

    /// Resolve a method to its immutable generated-entry contract, including
    /// a dynamically published source unit. The resolver returns metadata
    /// only; generated code performs the body invocation.
    #[allow(unsafe_code)]
    pub(crate) fn method_callable_plan(
        &self,
        class_name: &str,
        method_name: &str,
        object_target: bool,
    ) -> Option<NativeFixedCallablePlan> {
        if let Some(plan) =
            self.same_unit_method_callable_plan(class_name, method_name, object_target)
        {
            return Some(plan);
        }
        let requested = normalized_class_name(class_name);
        let normalized = unsafe { self.class_aliases.as_ref() }
            .and_then(|aliases| aliases.get(requested.as_ref()))
            .map_or(requested.as_ref(), String::as_str);
        let unit_index = unsafe { self.external_class_units.as_ref() }
            .and_then(|classes| classes.get(normalized).copied())?;
        let unit = self.dynamic_units()?.get(unit_index)?;
        let mut plan = native_compiled_method_callable_plan(
            &unit.compiled,
            normalized,
            method_name,
            object_target,
        )?;
        plan.runtime_view = std::ptr::from_ref(&*unit.published_runtime_view) as usize as u64;
        Some(plan)
    }

    /// Resolve the single PHP destructor target without invoking it. The
    /// generated release spine consumes the returned entry metadata directly;
    /// this capability performs no callback dispatch and no PHP execution.
    pub(crate) fn destructor_callable_plan(
        &self,
        class_name: &str,
    ) -> Option<NativeFixedCallablePlan> {
        let active = self.active_compiled()?;
        if let Some(plan) = native_compiled_destructor_callable_plan(active, class_name) {
            return Some(plan);
        }
        let requested = normalized_class_name(class_name);
        // SAFETY: the symbol-query capability points at request-owned maps for
        // the full generated release transition.
        #[allow(unsafe_code)]
        let normalized = unsafe { self.class_aliases.as_ref() }
            .and_then(|aliases| aliases.get(requested.as_ref()))
            .map_or(requested.as_ref(), String::as_str);
        #[allow(unsafe_code)]
        let unit_index = unsafe { self.external_class_units.as_ref() }
            .and_then(|classes| classes.get(normalized).copied())?;
        let unit = self.dynamic_units()?.get(unit_index)?;
        let mut plan = native_compiled_destructor_callable_plan(&unit.compiled, normalized)?;
        plan.runtime_view = std::ptr::from_ref(&*unit.published_runtime_view) as usize as u64;
        Some(plan)
    }

    /// Resolve a method across published unit boundaries with the PHP caller
    /// scope fixed by the generated callsite. This is required for inherited
    /// private/protected constructors: the target object's class and the
    /// declaring method can live in different dynamically published units.
    #[allow(unsafe_code)]
    pub(crate) fn scoped_method_callable_plan(
        &self,
        class_name: &str,
        method_name: &str,
        object_target: bool,
        caller_function: u32,
        active_view: &php_jit::JitNativeRuntimeView,
        root_runtime_view: u64,
    ) -> Option<NativeFixedCallablePlan> {
        fn generic_entries(compiled: &crate::compiled_unit::CompiledUnit) -> u64 {
            compiled
                .prepared_deployment_image()
                .generic_function_entries
                .as_ptr() as usize as u64
        }

        let active_compiled = self.active_compiled()?;
        let caller_compiled =
            if generic_entries(active_compiled) == active_view.trusted_generic_function_entries {
                active_compiled
            } else {
                let unit = self.dynamic_units()?.iter().find(|unit| {
                    unit.published_runtime_view.trusted_generic_function_entries
                        == active_view.trusted_generic_function_entries
                })?;
                &unit.compiled
            };
        let caller_class = caller_compiled.unit().classes.iter().find_map(|class| {
            class
                .methods
                .iter()
                .any(|method| method.function.raw() == caller_function)
                .then_some(class.name.as_str())
        });

        let locate = |name: &str| {
            let normalized = normalize_class_name(name);
            if let Some(class) = active_compiled.lookup_unit_class(&normalized) {
                return Some((active_compiled, class, root_runtime_view));
            }
            let unit_index = unsafe { self.external_class_units.as_ref() }
                .and_then(|classes| classes.get(&normalized).copied())?;
            let unit = self.dynamic_units()?.get(unit_index)?;
            let class = unit.compiled.lookup_unit_class(&normalized)?;
            let runtime_view =
                std::ptr::from_ref(unit.published_runtime_view.as_ref()) as usize as u64;
            Some((&unit.compiled, class, runtime_view))
        };

        let class_is_a = |child: &str, parent: &str| {
            let parent = normalize_class_name(parent);
            let mut candidate = normalize_class_name(child);
            for _ in 0..php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
                if candidate == parent {
                    return true;
                }
                let Some((_, class, _)) = locate(&candidate) else {
                    return false;
                };
                let Some(next) = class.parent.as_deref() else {
                    return false;
                };
                candidate = normalize_class_name(next);
            }
            false
        };

        let static_property_owner = |start: &str, property: &str| {
            let mut candidate = normalize_class_name(start);
            for _ in 0..php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
                let (_, class, runtime_view) = locate(&candidate)?;
                if class
                    .properties
                    .iter()
                    .any(|entry| entry.flags.is_static && entry.name == property)
                {
                    return Some((runtime_view, normalize_class_name(&class.name)));
                }
                candidate = normalize_class_name(class.parent.as_deref()?);
            }
            None
        };

        let mut candidate = normalize_class_name(class_name);
        for _ in 0..php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
            let (compiled, class, runtime_view) = locate(&candidate)?;
            if let Some(method) = class
                .methods
                .iter()
                .find(|method| method.name.eq_ignore_ascii_case(method_name))
            {
                if method.flags.is_abstract || (!object_target && !method.flags.is_static) {
                    return None;
                }
                let caller_has_access = if method.flags.is_private {
                    caller_class.is_some_and(|caller| {
                        normalize_class_name(caller) == normalize_class_name(&class.name)
                    })
                } else if method.flags.is_protected {
                    caller_class.is_some_and(|caller| class_is_a(caller, &class.name))
                } else {
                    true
                };
                if !caller_has_access {
                    return None;
                }
                let function = compiled.unit().functions.get(method.function.index())?;
                if native_exact_function_requires_non_reference_trampoline(function, true) {
                    return None;
                }
                for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                    let property = match &instruction.kind {
                        php_ir::InstructionKind::FetchStaticProperty {
                            class_name,
                            property,
                            ..
                        }
                        | php_ir::InstructionKind::AssignStaticProperty {
                            class_name,
                            property,
                            ..
                        }
                        | php_ir::InstructionKind::IssetStaticProperty {
                            class_name,
                            property,
                            ..
                        }
                        | php_ir::InstructionKind::EmptyStaticProperty {
                            class_name,
                            property,
                            ..
                        }
                        | php_ir::InstructionKind::IssetStaticPropertyDim {
                            class_name,
                            property,
                            ..
                        }
                        | php_ir::InstructionKind::EmptyStaticPropertyDim {
                            class_name,
                            property,
                            ..
                        }
                        | php_ir::InstructionKind::BindReferenceStaticProperty {
                            class_name,
                            property,
                            ..
                        }
                        | php_ir::InstructionKind::BindReferenceFromStaticPropertyDim {
                            class_name,
                            property,
                            ..
                        }
                        | php_ir::InstructionKind::UnsetStaticPropertyDim {
                            class_name,
                            property,
                            ..
                        } if class_name.eq_ignore_ascii_case("static") => property,
                        _ => continue,
                    };
                    if static_property_owner(class_name, property)
                        != static_property_owner(&class.name, property)
                    {
                        return None;
                    }
                }
                let has_receiver = !method.flags.is_static;
                let mut plan = native_fixed_callable_plan(compiled, method.function, has_receiver)?;
                if usize::from(has_receiver).saturating_add(plan.visible_arity as usize)
                    > u8::MAX as usize
                {
                    return None;
                }
                plan.runtime_view = runtime_view;
                return Some(plan);
            }
            candidate = normalize_class_name(class.parent.as_ref()?);
        }
        None
    }

    /// Decides callable visibility from published immutable class metadata.
    ///
    /// Public concrete methods and public magic dispatch are representation
    /// complete here. Visibility-sensitive, abstract, or unpublished class
    /// shapes return `None` so the callsite takes its single baseline
    /// continuation before producing an observable result.
    pub(crate) fn method_is_callable(
        &self,
        class_name: &str,
        method_name: &str,
        object_target: bool,
    ) -> Option<bool> {
        let mut candidate = normalize_class_name(class_name);
        for _ in 0..php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
            let class = self.class_handle(&candidate)?;
            if let Some(method) = class
                .methods
                .iter()
                .find(|method| method.name.eq_ignore_ascii_case(method_name))
            {
                if method.flags.is_abstract || method.flags.is_private || method.flags.is_protected
                {
                    return None;
                }
                if !object_target && !method.flags.is_static {
                    return None;
                }
                return Some(true);
            }
            let magic_name = if object_target {
                "__call"
            } else {
                "__callStatic"
            };
            if let Some(magic) = class
                .methods
                .iter()
                .find(|method| method.name.eq_ignore_ascii_case(magic_name))
            {
                if magic.flags.is_abstract
                    || magic.flags.is_private
                    || magic.flags.is_protected
                    || (!object_target && !magic.flags.is_static)
                {
                    return None;
                }
                return Some(true);
            }
            let Some(parent) = class.parent.as_deref() else {
                return Some(false);
            };
            candidate = normalize_class_name(parent);
        }
        None
    }
}

fn native_compiled_method_callable_plan(
    compiled: &crate::compiled_unit::CompiledUnit,
    class_name: &str,
    method_name: &str,
    object_target: bool,
) -> Option<NativeFixedCallablePlan> {
    let mut candidate = normalize_class_name(class_name);
    loop {
        let class = compiled
            .unit()
            .classes
            .iter()
            .find(|class| class.name == candidate)?;
        if let Some(method) = class
            .methods
            .iter()
            .find(|method| method.name.eq_ignore_ascii_case(method_name))
        {
            if method.flags.is_abstract
                || method.flags.is_private
                || method.flags.is_protected
                || (!object_target && !method.flags.is_static)
            {
                return None;
            }
            let function = compiled.unit().functions.get(method.function.index())?;
            if native_exact_function_requires_non_reference_trampoline(function, true) {
                return None;
            }
            let has_receiver = !method.flags.is_static;
            let plan = native_fixed_callable_plan(compiled, method.function, has_receiver)?;
            if usize::from(has_receiver).saturating_add(plan.visible_arity as usize)
                > u8::MAX as usize
            {
                return None;
            }
            return Some(plan);
        }
        candidate = normalize_class_name(class.parent.as_ref()?);
    }
}

/// Destructors are entered by the generated final-owner release spine, not by
/// an ordinary visibility-checked method call. Their runtime class scope and
/// zero-argument trace are already published by that spine, so the generic
/// method-call trampoline restrictions do not apply here.
fn native_compiled_destructor_callable_plan(
    compiled: &crate::compiled_unit::CompiledUnit,
    class_name: &str,
) -> Option<NativeFixedCallablePlan> {
    let mut candidate = normalize_class_name(class_name);
    loop {
        let class = compiled
            .unit()
            .classes
            .iter()
            .find(|class| class.name == candidate)?;
        if let Some(method) = class
            .methods
            .iter()
            .find(|method| method.name.eq_ignore_ascii_case("__destruct"))
        {
            if method.flags.is_abstract || method.flags.is_static {
                return None;
            }
            let plan = native_fixed_callable_plan(compiled, method.function, true)?;
            return (plan.visible_arity == 0).then_some(plan);
        }
        candidate = normalize_class_name(class.parent.as_ref()?);
    }
}

pub(crate) fn native_fixed_callable_plan(
    compiled: &crate::compiled_unit::CompiledUnit,
    function_id: php_ir::FunctionId,
    has_receiver: bool,
) -> Option<NativeFixedCallablePlan> {
    let function = compiled.unit().functions.get(function_id.index())?;
    let mut parameter_by_reference = [0_u64; 4];
    for (index, parameter) in function.params.iter().enumerate() {
        if parameter.by_ref
            && let Some(word) = parameter_by_reference.get_mut(index / 64)
        {
            *word |= 1_u64 << (index % 64);
        }
    }
    let first_parameter_by_reference = parameter_by_reference[0] & 1 != 0;
    let admitted = !function.flags.is_generator && function.params.len() <= u8::MAX as usize;
    let visible_arity = u32::try_from(function.params.len()).ok()?;
    let binding_plan =
        compiled.prepared_native_function_binding_plan_ptr(function_id)? as usize as u64;
    admitted.then(|| NativeFixedCallablePlan {
        function: function_id,
        runtime_view: 0,
        binding_plan,
        visible_arity,
        parameter_by_reference,
        has_receiver,
        first_parameter_by_reference,
        variadic: function
            .params
            .last()
            .is_some_and(|parameter| parameter.variadic),
        direct_packed_binding: function
            .params
            .iter()
            .all(|parameter| parameter.type_.is_none()),
        returns_by_reference: function.returns_by_ref,
        returns_int: matches!(
            function.return_type.as_ref(),
            Some(php_ir::IrReturnType::Int)
        ),
        returns_string: matches!(
            function.return_type.as_ref(),
            Some(php_ir::IrReturnType::String)
        ),
        returns_releasable_scalar: function
            .return_type
            .as_ref()
            .is_some_and(native_callback_return_type_is_releasable_scalar),
        magic_dispatch: false,
    })
}

fn native_callback_return_type_is_releasable_scalar(type_: &php_ir::IrReturnType) -> bool {
    use php_ir::IrReturnType as Type;
    match type_ {
        Type::Int
        | Type::Float
        | Type::String
        | Type::Bool
        | Type::Null
        | Type::False
        | Type::True
        | Type::Void
        | Type::Never => true,
        Type::Nullable { inner } => native_callback_return_type_is_releasable_scalar(inner),
        Type::Union { members } => {
            !members.is_empty()
                && members
                    .iter()
                    .all(native_callback_return_type_is_releasable_scalar)
        }
        Type::Array
        | Type::Callable
        | Type::Iterable
        | Type::Object
        | Type::Mixed
        | Type::Class { .. }
        | Type::Intersection { .. }
        | Type::Dnf { .. } => false,
    }
}

impl NativeRequestQueryCapability {
    #[allow(unsafe_code)]
    pub(crate) fn environment(&self) -> Option<&[(String, String)]> {
        unsafe { self.environment.as_ref() }.map(|environment| environment.as_ref().as_slice())
    }

    #[allow(unsafe_code)]
    pub(crate) fn included_files(&self) -> Option<&std::collections::BTreeSet<std::path::PathBuf>> {
        unsafe { self.included_files.as_ref() }
    }

    #[allow(unsafe_code)]
    pub(crate) fn sapi_name(&self) -> Option<&str> {
        unsafe { self.sapi_name.as_ref() }.map(String::as_str)
    }
}

impl NativeConfigurationCapability {
    /// Returns the request registry guaranteed by capability publication.
    ///
    /// Exact handlers never validate this engine invariant per invocation:
    /// `NativeRequestOwner` publishes the stable non-null owner before native
    /// execution can observe the fast state.
    #[allow(unsafe_code)]
    pub(crate) fn ini_registry(&self) -> &php_runtime::api::IniRegistry {
        unsafe { &*self.ini_registry }
    }

    #[allow(unsafe_code)]
    pub(crate) fn ini_registry_mut(&mut self) -> &mut php_runtime::api::IniRegistry {
        unsafe { &mut *self.ini_registry }
    }

    #[allow(unsafe_code)]
    pub(crate) fn include_path_mut(&mut self) -> &mut Arc<Vec<std::path::PathBuf>> {
        unsafe { &mut *self.include_path }
    }

    #[allow(unsafe_code)]
    pub(crate) fn include_path(&self) -> &Arc<Vec<std::path::PathBuf>> {
        unsafe { &*self.include_path }
    }

    #[allow(unsafe_code)]
    pub(crate) fn display_errors_mut(&mut self) -> &mut bool {
        unsafe { &mut *self.display_errors }
    }

    #[allow(unsafe_code)]
    pub(crate) fn default_timezone(&self) -> &str {
        unsafe { &*self.default_timezone }.as_str()
    }

    #[allow(unsafe_code)]
    pub(crate) fn default_timezone_mut(&mut self) -> &mut String {
        unsafe { &mut *self.default_timezone }
    }
}

impl NativeHttpResponseCapability {
    /// Publication guarantees the stable non-null owner; exact invocation
    /// therefore performs no repeated engine-integrity validation.
    #[allow(unsafe_code)]
    pub(crate) fn response(&self) -> &php_runtime::api::RuntimeHttpResponseState {
        unsafe { &*self.response }
    }

    #[allow(unsafe_code)]
    pub(crate) fn response_mut(&mut self) -> &mut php_runtime::api::RuntimeHttpResponseState {
        unsafe { &mut *self.response }
    }
}

impl NativeSessionCapability {
    #[allow(unsafe_code)]
    pub(crate) fn control(&self) -> &php_runtime::api::NativeSessionControlState {
        unsafe { &*self.control }
    }

    #[allow(unsafe_code)]
    pub(crate) fn control_mut(&mut self) -> &mut php_runtime::api::NativeSessionControlState {
        unsafe { &mut *self.control }
    }

    pub(crate) const fn has_loader(&self) -> bool {
        self.has_loader != 0
    }

    pub(crate) const fn has_id_generator(&self) -> bool {
        self.has_id_generator != 0
    }
}

impl NativeExecutionDeadlineCapability {
    /// Applies PHP's request-local `set_time_limit` semantics without exposing
    /// the cold request coordinator. A server-disabled deadline remains
    /// disabled even when PHP requests a positive limit.
    #[allow(unsafe_code)]
    pub(crate) fn reset_seconds(&mut self, seconds: u64) -> bool {
        let Some(deadline) = (unsafe { self.deadline.as_mut() }) else {
            return false;
        };
        if self.mutable == 0 {
            return true;
        }
        *deadline = if seconds == 0 {
            None
        } else {
            std::time::Instant::now().checked_add(std::time::Duration::from_secs(seconds))
        };
        true
    }

    /// Checks and publishes only the deadline diagnostic owned by this
    /// capability. No value plane, call frame, unit, or compatibility state
    /// is reachable from the exact poll.
    #[allow(unsafe_code)]
    pub(crate) fn poll(&mut self) -> i32 {
        let Some(deadline) = (unsafe { self.deadline.as_ref() }) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        if deadline.is_none_or(|deadline| std::time::Instant::now() < deadline) {
            return 0;
        }
        let Some(diagnostic) = (unsafe { self.diagnostic.as_mut() }) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        *diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
            "E_PHP_VM_EXECUTION_TIMEOUT",
            php_runtime::api::RuntimeSeverity::RecoverableError,
            "maximum execution time exceeded",
            php_runtime::api::RuntimeSourceSpan::default(),
            Vec::new(),
            None,
        ));
        php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
    }
}

impl NativeFrameArenaCapability {
    /// Allocates one generated frame from the authoritative native arena.
    ///
    /// Publication guarantees both pointers are valid for the synchronous
    /// request lifetime, so the compiled boundary performs no cold-context
    /// recovery or repeated engine-integrity validation.
    #[allow(unsafe_code)]
    pub(crate) fn allocate(&mut self, bytes: u64, alignment: u64) -> u64 {
        let result = usize::try_from(bytes)
            .map_err(|_| "E_PHP_VM_NATIVE_FRAME_LIMIT: frame size does not fit usize".to_owned())
            .and_then(|bytes| {
                usize::try_from(alignment)
                    .map_err(|_| {
                        "E_PHP_VM_NATIVE_FRAME_ALIGNMENT: alignment does not fit usize".to_owned()
                    })
                    .and_then(|alignment| unsafe { &mut *self.arena }.allocate(bytes, alignment))
            });
        match result {
            Ok(address) => address as u64,
            Err(message) => {
                unsafe {
                    *self.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                        "E_PHP_VM_NATIVE_FRAME_LIMIT",
                        php_runtime::api::RuntimeSeverity::FatalError,
                        message,
                        php_runtime::api::RuntimeSourceSpan::default(),
                        Vec::new(),
                        None,
                    ));
                }
                0
            }
        }
    }

    #[allow(unsafe_code)]
    pub(crate) fn release(&mut self, address: u64) -> i32 {
        match unsafe { &mut *self.arena }.release(address as usize) {
            Ok(()) => 0,
            Err(message) => {
                unsafe {
                    *self.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                        "E_PHP_VM_NATIVE_FRAME_ORDER",
                        php_runtime::api::RuntimeSeverity::FatalError,
                        message,
                        php_runtime::api::RuntimeSourceSpan::default(),
                        Vec::new(),
                        None,
                    ));
                }
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
            }
        }
    }
}
