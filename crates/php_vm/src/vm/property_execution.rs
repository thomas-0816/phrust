use super::prelude::*;

impl Vm {
    /// `isset($object->property)` for a declared/dynamic property, shared by the
    /// rich and dense executors. Returns `Ok(Value::Bool(_))`; a `__isset` magic
    /// method that throws (or an internal error) is returned as `Err` for the
    /// caller to propagate. Non-objects are not set, so they yield `false`.
    pub(super) fn isset_property_value(
        &self,
        compiled: &CompiledUnit,
        object: &Value,
        property: &str,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
        let Value::Object(object) = object else {
            return Ok(Value::Bool(false));
        };
        let value = property_state_value(compiled, state, stack, object, property);
        let result = if let Some(value) = value {
            !matches!(value, Value::Uninitialized | Value::Null)
        } else {
            match self.call_magic_property_method(
                compiled,
                object.clone(),
                "__isset",
                property,
                vec![CallArgument::positional(Value::String(
                    PhpString::from_test_str(property),
                ))],
                output,
                stack,
                state,
            ) {
                Ok(Some(value)) => match to_bool(&value) {
                    Ok(value) => value,
                    Err(message) => {
                        return Err(self.runtime_error(output, compiled, stack, message));
                    }
                },
                Ok(None) => false,
                Err(result) => return Err(result),
            }
        };
        Ok(Value::Bool(result))
    }

    /// `empty($object->property)` for a declared/dynamic property, shared by the
    /// rich and dense executors. Returns `Ok(Value::Bool(_))`; a throwing magic
    /// method (or an internal error) is returned as `Err`. Non-objects are empty.
    pub(super) fn empty_property_value(
        &self,
        compiled: &CompiledUnit,
        object: &Value,
        property: &str,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
        let Value::Object(object) = object else {
            return Ok(Value::Bool(true));
        };
        let result = match property_state_value(compiled, state, stack, object, property) {
            Some(value) => match php_empty_access_value(&value) {
                Ok(value) => value,
                Err(message) => return Err(self.runtime_error(output, compiled, stack, message)),
            },
            None => {
                let isset = match self.call_magic_property_method(
                    compiled,
                    object.clone(),
                    "__isset",
                    property,
                    vec![CallArgument::positional(Value::String(
                        PhpString::from_test_str(property),
                    ))],
                    output,
                    stack,
                    state,
                ) {
                    Ok(Some(value)) => match to_bool(&value) {
                        Ok(value) => value,
                        Err(message) => {
                            return Err(self.runtime_error(output, compiled, stack, message));
                        }
                    },
                    Ok(None) => false,
                    Err(result) => return Err(result),
                };
                if !isset {
                    true
                } else {
                    match self.call_magic_property_method(
                        compiled,
                        object.clone(),
                        "__get",
                        property,
                        vec![CallArgument::positional(Value::String(
                            PhpString::from_test_str(property),
                        ))],
                        output,
                        stack,
                        state,
                    ) {
                        Ok(Some(value)) => match php_empty_access_value(&value) {
                            Ok(value) => value,
                            Err(message) => {
                                return Err(self.runtime_error(output, compiled, stack, message));
                            }
                        },
                        Ok(None) => true,
                        Err(result) => return Err(result),
                    }
                }
            }
        };
        Ok(Value::Bool(result))
    }

    /// Shared instance-property unset: SPL array-as-props containers route
    /// through offsetUnset; declared properties validate visibility (with a
    /// `__unset` fallback) and honor typed-property reset semantics; unknown
    /// properties try `__unset` before removing dynamic storage. Both
    /// interpreters call this; the caller maps `Raise` into its own handler
    /// context.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn unset_property_value(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        property: &str,
        span: IrSpan,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), StaticPropertyAssignError> {
        if spl_array_object_uses_array_as_props(object) {
            return spl_container_offset_unset(
                object,
                &Value::String(PhpString::from_test_str(property)),
            )
            .map_err(StaticPropertyAssignError::Fatal);
        }
        let class = compiled.lookup_class(&object.class_name());
        let scope = current_scope_class(compiled, stack);
        let declared = match class {
            Some(class) => {
                match lookup_property_in_hierarchy(compiled, class, property, scope.as_deref()) {
                    Ok(property) => property,
                    Err(message) => return Err(StaticPropertyAssignError::Fatal(message)),
                }
            }
            None => None,
        };
        if let Some(resolved) = declared {
            if let Err(message) =
                validate_property_access(compiled, stack, resolved.class, resolved.property)
            {
                return match self.call_magic_property_method(
                    compiled,
                    object.clone(),
                    "__unset",
                    property,
                    vec![CallArgument::positional(Value::String(
                        PhpString::from_test_str(property),
                    ))],
                    output,
                    stack,
                    state,
                ) {
                    Ok(Some(_)) => Ok(()),
                    Ok(None) => Err(StaticPropertyAssignError::Raise(span, message)),
                    Err(result) => Err(StaticPropertyAssignError::Vm(Box::new(result))),
                };
            }
            if resolved.property.flags.is_static {
                emit_static_property_as_non_static_notice(
                    compiled,
                    output,
                    stack,
                    state,
                    resolved.class,
                    resolved.property,
                    span,
                );
                object.unset_property(property);
                return Ok(());
            }
            let storage_name = property_storage_name(resolved.class, resolved.property);
            if resolved.property.flags.is_typed {
                object.set_property(storage_name, Value::Uninitialized);
            } else {
                object.unset_property(&storage_name);
            }
            return Ok(());
        }
        match self.call_magic_property_method(
            compiled,
            object.clone(),
            "__unset",
            property,
            vec![CallArgument::positional(Value::String(
                PhpString::from_test_str(property),
            ))],
            output,
            stack,
            state,
        ) {
            Ok(Some(_)) | Ok(None) => {
                object.unset_property(property);
                Ok(())
            }
            Err(result) => Err(StaticPropertyAssignError::Vm(Box::new(result))),
        }
    }

    /// Shared static-property assignment: autoload, class/property
    /// resolution, static/visibility/type/readonly validation, and the
    /// lvalue write. Returns the assigned value plus the replaced effective
    /// value; the caller runs replaced-value destructors with its own
    /// handler context and stores the destination register.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn assign_static_property_value(
        &self,
        compiled: &CompiledUnit,
        class_name: &str,
        property: &str,
        value: Value,
        autoload_site: Option<(u64, FunctionId, BlockId, InstrId)>,
        span: IrSpan,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(Value, Value), StaticPropertyAssignError> {
        if let Err(result) = self.autoload_static_class_if_missing(
            compiled,
            class_name,
            span,
            autoload_site,
            output,
            stack,
            state,
        ) {
            return Err(StaticPropertyAssignError::Vm(Box::new(result)));
        }
        let class = resolve_static_class_name(compiled, state, stack, class_name)
            .map_err(|message| StaticPropertyAssignError::Raise(span, message))?;
        let scope = current_scope_class(compiled, stack);
        let resolved = match lookup_resolved_property_in_state(
            compiled,
            state,
            &class,
            property,
            scope.as_deref(),
        ) {
            Ok(Some(resolved)) => resolved,
            Ok(None) => {
                return Err(StaticPropertyAssignError::Raise(
                    span,
                    format!(
                        "E_PHP_VM_UNKNOWN_STATIC_PROPERTY: Access to undeclared static property {}::${property}",
                        class.display_name
                    ),
                ));
            }
            Err(message) => return Err(StaticPropertyAssignError::Fatal(message)),
        };
        if !resolved.property.flags.is_static {
            return Err(StaticPropertyAssignError::Fatal(format!(
                "E_PHP_VM_NON_STATIC_PROPERTY_ACCESS: property {}::${} is not static",
                resolved.class.name, resolved.property.name
            )));
        }
        if let Err(message) = validate_property_access_in_state(
            compiled,
            state,
            stack,
            &resolved.class,
            &resolved.property,
        ) {
            return Err(StaticPropertyAssignError::Fatal(message));
        }
        let property_type = ir_runtime_type(resolved.property.type_.as_ref());
        if let Err(message) = check_property_type(
            compiled,
            Some(state),
            resolved.class.display_name.as_str(),
            resolved.property.name.as_str(),
            &property_type,
            &value,
            self.typecheck_fast_path_context(),
        ) {
            return Err(StaticPropertyAssignError::Raise(span, message));
        }
        let key = static_property_key(&resolved.class, &resolved.property);
        let current = if let Some(value) = state.static_properties.get(&key) {
            value.clone()
        } else {
            match static_property_default(
                compiled,
                state,
                stack,
                &resolved.class,
                &resolved.property,
            ) {
                Ok(value) => value,
                Err(message) => return Err(StaticPropertyAssignError::Fatal(message)),
            }
        };
        if let Err(message) = validate_static_property_write(
            compiled,
            stack,
            &resolved.class,
            &resolved.property,
            &current,
        ) {
            return Err(StaticPropertyAssignError::Fatal(message));
        }
        let previous_effective = effective_value(&current);
        if let Err(message) =
            write_static_property_lvalue(&mut state.static_properties, key, current, value.clone())
        {
            return Err(StaticPropertyAssignError::Fatal(message));
        }
        Ok((value, previous_effective))
    }

    /// Resolves a `Class::$staticProperty` fetch to its value, shared by the
    /// rich and dense executors (mirrors `fetch_class_constant_value`). Faults
    /// are returned for the caller to route; `cache_site` supplies the
    /// inline-cache key.
    pub(super) fn fetch_static_property_value(
        &self,
        compiled: &CompiledUnit,
        class_name: &str,
        property: &str,
        cache_site: Option<(FunctionId, BlockId, InstrId)>,
        cache_id: Option<InlineCacheId>,
        span: IrSpan,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ClassConstantFetch> {
        let autoload_site = cache_site.map(|(function, block, instr)| {
            (compiled_unit_cache_key(compiled), function, block, instr)
        });
        if let Err(result) = self.autoload_static_class_if_missing(
            compiled,
            class_name,
            span,
            autoload_site,
            output,
            stack,
            state,
        ) {
            return Err(ClassConstantFetch::Throwable(Box::new(result)));
        }
        let class = match resolve_static_class_name(compiled, state, stack, class_name) {
            Ok(class) => class,
            Err(message) => return Err(ClassConstantFetch::Raise(span, message)),
        };
        let scope = current_scope_class(compiled, stack);
        let normalized_scope = scope.as_deref().map(normalize_class_name);
        let resolved_class = normalize_class_name(&class.name);
        let lookup_epoch = state.lookup_epoch();
        if let Some((function_id, block_id, instruction_id)) = cache_site
            && let Some(target) = self.lookup_class_constant_static_property_inline_cache(
                compiled,
                cache_id,
                function_id,
                block_id,
                instruction_id,
                ClassConstantStaticPropertyCacheKind::StaticProperty,
                &resolved_class,
                property,
                normalized_scope.as_deref(),
                lookup_epoch,
            )
        {
            match self.read_class_constant_static_property_target(compiled, target, stack, state) {
                Ok(ClassStaticCacheRead::Value(value)) => return Ok(value),
                Ok(ClassStaticCacheRead::Fallback) => {}
                Err(message) => return Err(ClassConstantFetch::Fatal(message)),
            }
        }
        let resolved = match lookup_resolved_property_in_state(
            compiled,
            state,
            &class,
            property,
            scope.as_deref(),
        ) {
            Ok(Some(resolved)) => resolved,
            Ok(None) => {
                let message = format!(
                    "E_PHP_VM_UNKNOWN_STATIC_PROPERTY: Access to undeclared static property {}::${property}",
                    class.display_name
                );
                return Err(ClassConstantFetch::Raise(span, message));
            }
            Err(message) => return Err(ClassConstantFetch::Fatal(message)),
        };
        if !resolved.property.flags.is_static {
            return Err(ClassConstantFetch::Fatal(format!(
                "E_PHP_VM_NON_STATIC_PROPERTY_ACCESS: property {}::${} is not static",
                resolved.class.name, resolved.property.name
            )));
        }
        if let Err(message) = validate_property_access_in_state(
            compiled,
            state,
            stack,
            &resolved.class,
            &resolved.property,
        ) {
            return Err(ClassConstantFetch::Fatal(message));
        }
        let key = static_property_key(&resolved.class, &resolved.property);
        if !state.static_properties.contains_key(&key) {
            let default = match static_property_default(
                compiled,
                state,
                stack,
                &resolved.class,
                &resolved.property,
            ) {
                Ok(value) => value,
                Err(message) => return Err(ClassConstantFetch::Fatal(message)),
            };
            state.static_properties.insert(key.clone(), default);
        }
        let value = state
            .static_properties
            .get(&key)
            .cloned()
            .unwrap_or(Value::Uninitialized);
        if matches!(value, Value::Uninitialized) {
            let message = format!(
                "E_PHP_VM_UNINITIALIZED_STATIC_PROPERTY: typed static property {}::${} must not be accessed before initialization",
                resolved.class.display_name, resolved.property.name
            );
            return Err(ClassConstantFetch::Raise(span, message));
        }
        let cache_scope =
            if resolved.property.flags.is_private || resolved.property.flags.is_protected {
                normalized_scope.clone()
            } else {
                None
            };
        if let Some((function_id, block_id, instruction_id)) = cache_site {
            let target = match dynamic_class_owner_index_in_state(state, &resolved.class.name) {
                Some(unit_index) => ClassConstantStaticPropertyCacheTarget::DynamicUnit {
                    unit_index,
                    kind: ClassConstantStaticPropertyCacheKind::StaticProperty,
                    resolved_class: resolved_class.clone(),
                    declaring_class: resolved.class.name.clone(),
                    member: resolved.property.name.clone(),
                },
                None => ClassConstantStaticPropertyCacheTarget::CurrentUnit {
                    kind: ClassConstantStaticPropertyCacheKind::StaticProperty,
                    resolved_class: resolved_class.clone(),
                    declaring_class: resolved.class.name.clone(),
                    member: resolved.property.name.clone(),
                },
            };
            self.install_class_constant_static_property_inline_cache(
                compiled,
                cache_id,
                function_id,
                block_id,
                instruction_id,
                ClassConstantStaticPropertyCacheKind::StaticProperty,
                &resolved_class,
                property,
                cache_scope.as_deref(),
                lookup_epoch,
                target,
            );
        }
        Ok(value)
    }

    /// `$object->property[dims...] = value` (or `[] =` append), shared by the
    /// rich and dense executors. Extracted verbatim from the rich handler; only
    /// the exit paths are rewritten (produce -> Ok, raise -> Raise, ArrayAccess
    /// offsetSet result -> Return, internal error -> Fatal). The caller writes
    /// the returned value to its destination register.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn assign_property_dim_value(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        property: &str,
        dims: &[ArrayKey],
        append: bool,
        value: Value,
        span: IrSpan,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, PropertyDimAssign> {
        if is_std_class_runtime_class(&object.class_name()) {
            // In-place fast path: mutate an existing array
            // property directly instead of clone → assign
            // (COW separation) → write-back. stdClass has no
            // declared properties, hooks, or types; userland
            // ArrayAccess dispatch only applies to object
            // values, which stay on the generic path.
            match try_assign_property_dim_in_place(&object, property, dims, value.clone(), append) {
                PropertyDimInPlace::Applied(Ok(())) => {
                    self.record_counter_property_dim_assign_in_place_hit();
                    return Ok(value);
                }
                PropertyDimInPlace::Applied(Err(message)) => {
                    return Err(PropertyDimAssign::Raise(span, message));
                }
                PropertyDimInPlace::NotEligible => {
                    self.record_counter_property_dim_assign_generic("stdclass_non_array_slot");
                }
            }
            let mut current = object.get_property(property).unwrap_or(Value::Null);
            match self.try_userland_arrayaccess_offset_set_value(
                compiled,
                output,
                stack,
                state,
                &current,
                dims,
                append,
                value.clone(),
                span,
            ) {
                Ok(true) => return Ok(value),
                Ok(false) => {}
                Err(result) => return Err(PropertyDimAssign::Return(Box::new(result))),
            }
            if matches!(current, Value::Uninitialized | Value::Null) {
                current = Value::Array(PhpArray::new());
            }
            if let Err(message) = assign_dim_value(&mut current, dims, value.clone(), append) {
                return Err(PropertyDimAssign::Raise(span, message));
            }
            object.set_property(property, current);
            return Ok(value);
        }
        let class = match lookup_class_in_state(compiled, state, &object.class_name()) {
            Some(class) => class,
            None => {
                return Err(PropertyDimAssign::Fatal(format!(
                    "E_PHP_VM_UNKNOWN_CLASS: class {} is not defined",
                    object.class_name()
                )));
            }
        };
        let scope = current_scope_class(compiled, stack);
        let resolved = match lookup_resolved_property_in_state(
            compiled,
            state,
            &class,
            property,
            scope.as_deref(),
        ) {
            Ok(Some(resolved)) => resolved,
            Ok(None) => {
                // In-place fast path for an existing dynamic
                // array property. Gated on the class allowing
                // dynamic properties so the deprecation
                // diagnostic behavior of the generic path is
                // preserved exactly for disallowing classes.
                if class_allows_dynamic_properties(compiled, state, &class) {
                    match try_assign_property_dim_in_place(
                        &object,
                        property,
                        dims,
                        value.clone(),
                        append,
                    ) {
                        PropertyDimInPlace::Applied(Ok(())) => {
                            self.record_counter_property_dim_assign_in_place_hit();
                            return Ok(value);
                        }
                        PropertyDimInPlace::Applied(Err(message)) => {
                            return Err(PropertyDimAssign::Raise(span, message));
                        }
                        PropertyDimInPlace::NotEligible => {
                            self.record_counter_property_dim_assign_generic(
                                "dynamic_non_array_slot",
                            );
                        }
                    }
                } else {
                    self.record_counter_property_dim_assign_generic(
                        "dynamic_properties_disallowed",
                    );
                }
                let mut current = object.get_property(property).unwrap_or(Value::Null);
                match self.try_userland_arrayaccess_offset_set_value(
                    compiled,
                    output,
                    stack,
                    state,
                    &current,
                    dims,
                    append,
                    value.clone(),
                    span,
                ) {
                    Ok(true) => return Ok(value),
                    Ok(false) => {}
                    Err(result) => return Err(PropertyDimAssign::Return(Box::new(result))),
                }
                if matches!(current, Value::Uninitialized | Value::Null) {
                    current = Value::Array(PhpArray::new());
                }
                if let Err(message) = assign_dim_value(&mut current, dims, value.clone(), append) {
                    return Err(PropertyDimAssign::Raise(span, message));
                }
                if let Some(diagnostic) = dynamic_property_deprecation_diagnostic(
                    compiled, state, &class, &object, property, stack,
                ) {
                    diagnostics.push(diagnostic);
                }
                object.set_property(property, current);
                return Ok(value);
            }
            Err(message) => {
                return Err(PropertyDimAssign::Fatal(message));
            }
        };
        let resolved_class = &resolved.class;
        let entry = &resolved.property;
        if entry.flags.is_static {
            if let Err(message) =
                validate_property_access_in_state(compiled, state, stack, resolved_class, entry)
                    .and_then(|()| {
                        validate_property_set_access_in_state(
                            compiled,
                            state,
                            stack,
                            resolved_class,
                            entry,
                        )
                    })
            {
                return Err(PropertyDimAssign::Raise(span, message));
            }
            emit_static_property_as_non_static_notice(
                compiled,
                output,
                stack,
                state,
                resolved_class,
                entry,
                span,
            );
            let mut current = object.get_property(property).unwrap_or(Value::Null);
            match self.try_userland_arrayaccess_offset_set_value(
                compiled,
                output,
                stack,
                state,
                &current,
                dims,
                append,
                value.clone(),
                span,
            ) {
                Ok(true) => return Ok(value),
                Ok(false) => {}
                Err(result) => return Err(PropertyDimAssign::Return(Box::new(result))),
            }
            if matches!(current, Value::Uninitialized | Value::Null) {
                current = Value::Array(PhpArray::new());
            }
            if let Err(message) = assign_dim_value(&mut current, dims, value.clone(), append) {
                return Err(PropertyDimAssign::Raise(span, message));
            }
            object.set_property(property, current);
            return Ok(value);
        }
        if let Err(message) =
            validate_property_access_in_state(compiled, state, stack, resolved_class, entry)
                .and_then(|()| {
                    validate_property_set_access_in_state(
                        compiled,
                        state,
                        stack,
                        resolved_class,
                        entry,
                    )
                })
        {
            return Err(PropertyDimAssign::Raise(span, message));
        }
        if entry.hooks.get.is_some() || entry.hooks.set.is_some() {
            return Err(PropertyDimAssign::Fatal(format!(
                "E_PHP_VM_PROPERTY_DIM_HOOK: property {}::${} dimension assignment through hooks is not implemented",
                resolved_class.name, entry.name
            )));
        }
        let storage_name = property_storage_name(resolved_class, entry);
        // In-place fast path: untyped, non-readonly declared
        // property (visibility validated above, hooks
        // excluded) whose storage slot currently holds an
        // array. Typed properties keep the generic path so
        // post-assignment type checks stay exact; readonly
        // properties keep the generic error ordering.
        if entry.type_.is_none() && !entry.flags.is_readonly && !resolved_class.flags.is_readonly {
            match try_assign_property_dim_in_place(
                &object,
                &storage_name,
                dims,
                value.clone(),
                append,
            ) {
                PropertyDimInPlace::Applied(Ok(())) => {
                    self.record_counter_property_dim_assign_in_place_hit();
                    return Ok(value);
                }
                PropertyDimInPlace::Applied(Err(message)) => {
                    return Err(PropertyDimAssign::Raise(span, message));
                }
                PropertyDimInPlace::NotEligible => {
                    self.record_counter_property_dim_assign_generic("declared_non_array_slot");
                }
            }
        } else {
            self.record_counter_property_dim_assign_generic("typed_readonly_or_readonly_class");
        }
        let mut current =
            property_state_value(compiled, state, stack, &object, property).unwrap_or(Value::Null);
        match self.try_userland_arrayaccess_offset_set_value(
            compiled,
            output,
            stack,
            state,
            &current,
            dims,
            append,
            value.clone(),
            span,
        ) {
            Ok(true) => return Ok(value),
            Ok(false) => {}
            Err(result) => return Err(PropertyDimAssign::Return(Box::new(result))),
        }
        if matches!(current, Value::Uninitialized | Value::Null) {
            current = Value::Array(PhpArray::new());
        }
        if let Err(message) = assign_dim_value(&mut current, dims, value.clone(), append) {
            return Err(PropertyDimAssign::Raise(span, message));
        }
        let property_type = ir_runtime_type(entry.type_.as_ref());
        if let Err(message) = check_property_type(
            compiled,
            Some(state),
            resolved_class.display_name.as_str(),
            property,
            &property_type,
            &current,
            self.typecheck_fast_path_context(),
        ) {
            return Err(PropertyDimAssign::Raise(span, message));
        }
        if let Err(message) =
            validate_property_write(resolved_class, entry, &object, stack, compiled)
        {
            return Err(PropertyDimAssign::Raise(span, message));
        }
        object.set_property(storage_name, current);
        Ok(value)
    }
}
