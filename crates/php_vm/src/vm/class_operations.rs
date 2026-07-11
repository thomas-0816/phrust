//! Class lookup, construction, introspection, and autoload operations.

use super::prelude::*;

impl Vm {
    pub(super) fn fetch_class_constant_value(
        &self,
        compiled: &CompiledUnit,
        class_name: &str,
        constant: &str,
        cache_site: Option<(FunctionId, BlockId, InstrId)>,
        cache_id: Option<InlineCacheId>,
        span: IrSpan,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ClassConstantFetch> {
        if constant.eq_ignore_ascii_case("class")
            && normalize_class_name(class_name) == "static"
            && let Some(called_class) = current_called_class_display(compiled, stack)
        {
            return Ok(Value::String(PhpString::from_test_str(&called_class)));
        }
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
        let value = if constant.eq_ignore_ascii_case("class") {
            Value::String(PhpString::from_test_str(&class.display_name))
        } else if let Some(value) = pdo_class_constant_value(&class.name, constant) {
            value
        } else if let Some(value) = zip_class_constant_value(&class.name, constant) {
            value
        } else if let Some(value) = redis_class_constant_value(&class.name, constant) {
            value
        } else if let Some(value) = memcached_class_constant_value(&class.name, constant) {
            value
        } else if let Some(value) = intl_class_constant_value(&class.name, constant) {
            value
        } else if let Some(value) = xml_class_constant_value(&class.name, constant) {
            value
        } else if let Some(value) = date_time_class_constant_value(&class.name, constant) {
            value
        } else if let Some(value) =
            spl_class_constant_value_in_state(compiled, state, &class.name, constant)
        {
            value
        } else {
            let scope = current_scope_class(compiled, stack);
            let normalized_scope = scope.as_deref().map(normalize_class_name);
            let resolved_class = normalize_class_name(&class.name);
            let lookup_epoch = state.lookup_epoch();
            let cache_kind = if class.flags.is_enum
                && class
                    .enum_cases
                    .iter()
                    .any(|case| case.name.eq_ignore_ascii_case(constant))
            {
                ClassConstantStaticPropertyCacheKind::EnumCase
            } else {
                ClassConstantStaticPropertyCacheKind::ClassConstant
            };
            if let Some((function_id, block_id, instruction_id)) = cache_site
                && let Some(target) = self.lookup_class_constant_static_property_inline_cache(
                    compiled,
                    cache_id,
                    function_id,
                    block_id,
                    instruction_id,
                    cache_kind,
                    &resolved_class,
                    constant,
                    normalized_scope.as_deref(),
                    lookup_epoch,
                )
            {
                match self
                    .read_class_constant_static_property_target(compiled, target, stack, state)
                {
                    Ok(ClassStaticCacheRead::Value(value)) => return Ok(value),
                    Ok(ClassStaticCacheRead::Fallback) => {}
                    Err(message) => return Err(ClassConstantFetch::Fatal(message)),
                }
            }
            if class.flags.is_enum
                && let Some(case) = class
                    .enum_cases
                    .iter()
                    .find(|case| case.name.eq_ignore_ascii_case(constant))
            {
                let owner = class_owner_in_state(compiled, state, &class.name);
                let object = match enum_case_object(&owner, state, &class, case, &|value| {
                    self.constant_value(owner.unit(), value)
                }) {
                    Ok(object) => object,
                    Err(message) => return Err(ClassConstantFetch::Fatal(message)),
                };
                if let Some((function_id, block_id, instruction_id)) = cache_site {
                    let target = match dynamic_class_owner_index_in_state(state, &class.name) {
                        Some(unit_index) => ClassConstantStaticPropertyCacheTarget::DynamicUnit {
                            unit_index,
                            kind: ClassConstantStaticPropertyCacheKind::EnumCase,
                            resolved_class: resolved_class.clone(),
                            declaring_class: class.name.clone(),
                            member: case.name.clone(),
                        },
                        None => ClassConstantStaticPropertyCacheTarget::CurrentUnit {
                            kind: ClassConstantStaticPropertyCacheKind::EnumCase,
                            resolved_class: resolved_class.clone(),
                            declaring_class: class.name.clone(),
                            member: case.name.clone(),
                        },
                    };
                    self.install_class_constant_static_property_inline_cache(
                        compiled,
                        cache_id,
                        function_id,
                        block_id,
                        instruction_id,
                        ClassConstantStaticPropertyCacheKind::EnumCase,
                        &resolved_class,
                        constant,
                        None,
                        lookup_epoch,
                        target,
                    );
                }
                return Ok(Value::Object(object));
            }
            let (resolved_class_entry, resolved_constant_entry) =
                match lookup_class_constant_in_state(
                    compiled,
                    state,
                    &class.name,
                    &class.display_name,
                    constant,
                ) {
                    Ok(Some(resolved)) => resolved,
                    Ok(None) => {
                        let message = format!(
                            "E_PHP_VM_UNKNOWN_CLASS_CONSTANT: Undefined constant {}::{constant}",
                            class.display_name
                        );
                        return Err(ClassConstantFetch::Raise(span, message));
                    }
                    Err(message) => return Err(ClassConstantFetch::Fatal(message)),
                };
            if let Err(message) = validate_constant_access(
                compiled,
                stack,
                &resolved_class_entry,
                &resolved_constant_entry,
            ) {
                return Err(ClassConstantFetch::Raise(span, message));
            }
            let cache_scope = if resolved_constant_entry.flags.is_private
                || resolved_constant_entry.flags.is_protected
            {
                normalized_scope.clone()
            } else {
                None
            };
            let owner = class_owner_in_state(compiled, state, &resolved_class_entry.name);
            let value = match resolved_constant_entry.value {
                Some(value) => match constant_value(owner.unit(), value) {
                    Ok(value) => value,
                    Err(message) => return Err(ClassConstantFetch::Fatal(message)),
                },
                None => {
                    if let Some(reference) = &resolved_constant_entry.value_class_constant {
                        match class_constant_reference_value(compiled, state, reference) {
                            Ok(value) => value,
                            Err(message) => return Err(ClassConstantFetch::Fatal(message)),
                        }
                    } else if let Some(reference) = &resolved_constant_entry.value_named_constant {
                        match named_constant_reference_value(compiled, state, reference) {
                            Ok(value) => value,
                            Err(message) => return Err(ClassConstantFetch::Fatal(message)),
                        }
                    } else {
                        Value::Null
                    }
                }
            };
            if let Some((function_id, block_id, instruction_id)) = cache_site {
                let target =
                    match dynamic_class_owner_index_in_state(state, &resolved_class_entry.name) {
                        Some(unit_index) => ClassConstantStaticPropertyCacheTarget::DynamicUnit {
                            unit_index,
                            kind: ClassConstantStaticPropertyCacheKind::ClassConstant,
                            resolved_class: resolved_class.clone(),
                            declaring_class: resolved_class_entry.name.clone(),
                            member: resolved_constant_entry.name.clone(),
                        },
                        None => ClassConstantStaticPropertyCacheTarget::CurrentUnit {
                            kind: ClassConstantStaticPropertyCacheKind::ClassConstant,
                            resolved_class: resolved_class.clone(),
                            declaring_class: resolved_class_entry.name.clone(),
                            member: resolved_constant_entry.name.clone(),
                        },
                    };
                self.install_class_constant_static_property_inline_cache(
                    compiled,
                    cache_id,
                    function_id,
                    block_id,
                    instruction_id,
                    ClassConstantStaticPropertyCacheKind::ClassConstant,
                    &resolved_class,
                    constant,
                    cache_scope.as_deref(),
                    lookup_epoch,
                    target,
                );
            }
            value
        };
        Ok(value)
    }

    /// `clone $object`, shared by the rich and dense executors. Returns the new
    /// object; a throwing `__clone` is returned as `ClassConstantFetch::Throwable`
    /// for the caller to route, other failures as `Fatal`. (`ClassConstantFetch`
    /// is the shared opcode-fault type for these helpers.)
    pub(super) fn clone_object_value(
        &self,
        compiled: &CompiledUnit,
        object: &Value,
        span: IrSpan,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ClassConstantFetch> {
        let Value::Object(object) = object else {
            return Err(ClassConstantFetch::Fatal(format!(
                "E_PHP_VM_CLONE_NON_OBJECT: cannot clone {}",
                value_type_name(object)
            )));
        };
        if self.is_finalized_hash_context_object(object) {
            return Err(ClassConstantFetch::Raise(
                span,
                "E_PHP_VM_SPL_ERROR: Cannot clone a finalized HashContext".to_owned(),
            ));
        }
        let class = match lookup_class_in_state(compiled, state, &object.class_name()) {
            Some(class) => class,
            None if internal_runtime_class_entry(&object.class_name()).is_some() => {
                return Ok(Value::Object(object.clone_shallow()));
            }
            None => {
                return Err(ClassConstantFetch::Fatal(format!(
                    "E_PHP_VM_UNKNOWN_CLASS: class {} is not defined",
                    object.class_name()
                )));
            }
        };
        let class_owner = class_owner_in_state(compiled, state, &class.name);
        let runtime_class = match runtime_class_entry(
            &class_owner,
            state,
            &class,
            &|value| self.constant_value(class_owner.unit(), value),
            &|reference| class_constant_reference_value(&class_owner, state, reference),
            &|reference| named_constant_reference_value(&class_owner, state, reference),
        ) {
            Ok(class) => class,
            Err(message) => return Err(ClassConstantFetch::Fatal(message.into())),
        };
        if let Err(message) = validate_object_mvp(&runtime_class) {
            return Err(ClassConstantFetch::Fatal(message));
        }
        let copy = match self.clone_object_with_magic(
            compiled,
            object.clone(),
            &class,
            output,
            stack,
            state,
        ) {
            Ok(copy) => copy,
            Err(result) => return Err(ClassConstantFetch::Throwable(Box::new(result))),
        };
        self.register_destructor_if_needed(compiled, &class, copy.clone(), state);
        Ok(Value::Object(copy))
    }

    pub(super) fn is_finalized_hash_context_object(&self, object: &ObjectRef) -> bool {
        object.class_name().eq_ignore_ascii_case("HashContext")
            && matches!(
                object.get_property("__phrust_hash_finalized"),
                Some(Value::Bool(true))
            )
    }

    pub(super) fn spl_autoload_candidate_exists(
        &self,
        compiled: &CompiledUnit,
        path: &str,
        stack: &CallStack,
        state: &ExecutionState,
    ) -> bool {
        let Some(loader) = &self.options.include_loader else {
            return false;
        };
        let including_file = current_source_path(compiled, stack);
        let include_path = state_include_path(state);
        let cwd = state.cwd.clone();
        if let Some(cache) = &self.options.include_cache {
            cache
                .resolve_with_include_path(
                    loader,
                    including_file.as_deref(),
                    path,
                    &include_path,
                    Some(&cwd),
                )
                .is_ok()
        } else {
            loader
                .resolve_with_include_path(
                    including_file.as_deref(),
                    path,
                    &include_path,
                    Some(&cwd),
                )
                .is_ok()
        }
    }

    pub(super) fn execute_spl_autoload(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        call_site: Option<(u64, FunctionId, BlockId, InstrId)>,
        call_span: Option<IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if values.is_empty() || values.len() > 2 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_AUTOLOAD_ARITY: spl_autoload expects one or two arguments",
            );
        }
        let class_name = match to_string(&values[0]) {
            Ok(name) => name.to_string_lossy(),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let extensions = match values.get(1) {
            Some(Value::Null) => String::new(),
            Some(value) => match to_string(value) {
                Ok(value) => value.to_string_lossy(),
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            },
            None => state.spl_autoload_extensions.clone(),
        };
        if extensions.is_empty() {
            return VmResult::success_no_output(Some(Value::Null));
        }

        let base = class_name.to_ascii_lowercase();
        let (unit_key, function_id, block_id, instruction_id) = call_site.unwrap_or_else(|| {
            (
                compiled_unit_cache_key(compiled),
                compiled.unit().entry,
                BlockId::new(0),
                InstrId::new(0),
            )
        });
        let span = call_span.unwrap_or_else(|| IrSpan::new(php_ir::FileId::new(0), 0, 0));
        for extension in extensions.split(',') {
            let path = format!("{base}{extension}");
            if !self.spl_autoload_candidate_exists(compiled, &path, stack, state) {
                continue;
            }
            let result = self.execute_include(
                compiled,
                None,
                unit_key,
                function_id,
                block_id,
                instruction_id,
                span,
                IncludeKind::IncludeOnce,
                &Value::string(path),
                output,
                stack,
                state,
            );
            if !result.status.is_success() {
                return result;
            }
            if lookup_class_in_state(compiled, state, &class_name).is_some() {
                break;
            }
        }
        VmResult::success_no_output(Some(Value::Null))
    }

    pub(super) fn call_autoload_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        args: Vec<CallArgument>,
        call_site: Option<(u64, FunctionId, BlockId, InstrId)>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if name == "is_callable" {
            return self.call_is_callable_builtin(compiled, args, output, stack, state);
        }
        let values = match call_args_to_positional(name, args) {
            Ok(values) => values,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        match name {
            "spl_autoload_register" => {
                let callback = if let Some(callback) = values.first() {
                    match autoload_callback_from_value(compiled, state, callback.clone()) {
                        Ok(callback) => callback,
                        Err(message) => {
                            return self.runtime_error(
                                output,
                                compiled,
                                stack,
                                autoload_invalid_callback_error("spl_autoload_register", &message),
                            );
                        }
                    }
                } else {
                    CallableValue::InternalBuiltin {
                        name: "spl_autoload".to_owned(),
                    }
                };
                if matches!(
                    &callback,
                    CallableValue::InternalBuiltin { name } if name == "spl_autoload_call"
                ) {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_SPL_VALUE_ERROR: spl_autoload_register(): Argument #1 ($callback) must not be the spl_autoload_call() function",
                    );
                }
                if values
                    .get(1)
                    .is_some_and(|do_throw| !to_bool(do_throw).unwrap_or(true))
                    && let Err(result) = self.emit_spl_autoload_register_do_throw_notice(
                        compiled, call_span, output, stack, state,
                    )
                {
                    return result;
                }
                let prepend = values
                    .get(2)
                    .and_then(|value| to_bool(value).ok())
                    .unwrap_or(false);
                state
                    .autoload_registry
                    .register_with_prepend(callback, prepend);
                state.bump_autoload_stack_epoch();
                VmResult::success_no_output(Some(Value::Bool(true)))
            }
            "spl_autoload_unregister" => {
                let Some(callback) = values.first() else {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_AUTOLOAD_ARITY: spl_autoload_unregister expects at least 1 argument",
                    );
                };
                let callback = match autoload_callback_from_value(compiled, state, callback.clone())
                {
                    Ok(callback) => callback,
                    Err(message) => {
                        return self.runtime_error(
                            output,
                            compiled,
                            stack,
                            autoload_invalid_callback_error("spl_autoload_unregister", &message),
                        );
                    }
                };
                if matches!(
                    &callback,
                    CallableValue::InternalBuiltin { name } if name == "spl_autoload_call"
                ) {
                    if let Err(result) = self.emit_spl_autoload_call_unregister_deprecation(
                        compiled, call_span, output, stack, state,
                    ) {
                        return result;
                    }
                    state.autoload_registry.clear();
                    state.bump_autoload_stack_epoch();
                    return VmResult::success_no_output(Some(Value::Bool(true)));
                }
                let removed = state.autoload_registry.unregister(&callback);
                if removed {
                    state.bump_autoload_stack_epoch();
                }
                VmResult::success_no_output(Some(Value::Bool(removed)))
            }
            "spl_autoload_functions" => {
                let mut array = PhpArray::new();
                for callback in state.autoload_registry.callbacks() {
                    array.append(autoload_callback_public_value(compiled, state, callback));
                }
                VmResult::success_no_output(Some(Value::Array(array)))
            }
            "spl_autoload_call" => {
                let Some(class_name) = values.first() else {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_AUTOLOAD_ARITY: spl_autoload_call expects class name",
                    );
                };
                let class_name = match to_string(class_name) {
                    Ok(name) => name.to_string_lossy(),
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                match self.autoload_class(compiled, &class_name, output, stack, state, None) {
                    Ok(()) => VmResult::success_no_output(Some(Value::Null)),
                    Err(result) => result,
                }
            }
            "spl_autoload_extensions" => {
                if values.len() > 1 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_AUTOLOAD_ARITY: spl_autoload_extensions expects zero or one argument",
                    );
                }
                if let Some(extensions) = values.first() {
                    let extensions = match to_string(extensions) {
                        Ok(value) => value.to_string_lossy(),
                        Err(message) => {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    };
                    state.spl_autoload_extensions = extensions;
                }
                VmResult::success_no_output(Some(Value::string(
                    state.spl_autoload_extensions.clone(),
                )))
            }
            "spl_autoload" => self
                .execute_spl_autoload(compiled, values, call_site, call_span, output, stack, state),
            "defined" => {
                let Some(constant_name) = values.first() else {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_SYMBOL_ARITY: defined expects constant name",
                    );
                };
                let constant_name = match to_string(constant_name) {
                    Ok(name) => name.to_string_lossy(),
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                VmResult::success_no_output(Some(Value::Bool(matches!(
                    global_constant_value(compiled, state, stack, &constant_name),
                    Ok(Some(_))
                ))))
            }
            "define" => {
                if values.len() < 2 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_SYMBOL_ARITY: define expects name and value",
                    );
                }
                let constant_name = match to_string(&values[0]) {
                    Ok(name) => name.to_string_lossy(),
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                if constant_name.is_empty() {
                    return VmResult::success_no_output(Some(Value::Bool(false)));
                }
                if matches!(
                    global_constant_value(compiled, state, stack, &constant_name),
                    Ok(Some(_)) | Err(_)
                ) {
                    let mut diagnostics = Vec::new();
                    if let Err(result) = self.emit_define_duplicate_warning(
                        compiled,
                        call_span,
                        output,
                        stack,
                        state,
                        &mut diagnostics,
                        &constant_name,
                    ) {
                        return result;
                    }
                    return VmResult::success_with_diagnostics_no_output(
                        Some(Value::Bool(false)),
                        diagnostics,
                    );
                }
                state
                    .user_constants
                    .insert(constant_name, effective_value(&values[1]));
                state.bump_lookup_epoch();
                VmResult::success_no_output(Some(Value::Bool(true)))
            }
            "constant" => {
                let Some(constant_name) = values.first() else {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_SYMBOL_ARITY: constant expects constant name",
                    );
                };
                let constant_name = match to_string(constant_name) {
                    Ok(name) => name.to_string_lossy(),
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                let resolved = match global_constant_lookup(compiled, state, stack, &constant_name)
                {
                    Ok(Some(resolved)) => resolved,
                    Ok(None) => {
                        return self.runtime_error(
                            output,
                            compiled,
                            stack,
                            format!(
                                "E_PHP_RUNTIME_UNDEFINED_CONSTANT: undefined constant {constant_name}"
                            ),
                        );
                    }
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                let mut diagnostics = Vec::new();
                if let Some(constant) = resolved.predefined
                    && let Err(result) = self.emit_predefined_constant_deprecation(
                        compiled,
                        output,
                        stack,
                        state,
                        &mut diagnostics,
                        call_span.unwrap_or_else(|| IrSpan::new(php_ir::FileId::new(0), 0, 0)),
                        constant,
                    )
                {
                    return result;
                }
                VmResult::success_with_diagnostics_no_output(Some(resolved.value), diagnostics)
            }
            "extension_loaded" => {
                let Some(extension_name) = values.first() else {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_SYMBOL_ARITY: extension_loaded expects extension name",
                    );
                };
                let extension_name = match to_string(extension_name) {
                    Ok(name) => name.to_string_lossy(),
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                let registry = php_std::ExtensionRegistry::standard_library();
                VmResult::success_no_output(Some(Value::Bool(
                    php_std::introspection::extension_loaded(registry, &extension_name),
                )))
            }
            "function_exists" => {
                let Some(function_name) = values.first() else {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_SYMBOL_ARITY: function_exists expects function name",
                    );
                };
                let function_name = match to_string(function_name) {
                    Ok(name) => normalize_function_name(&name.to_string_lossy()),
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                let registry = php_std::ExtensionRegistry::standard_library();
                VmResult::success_no_output(Some(Value::Bool(
                    compiled.lookup_function(&function_name).is_some()
                        || dynamic_function_in_state(state, &function_name).is_some()
                        || BuiltinRegistry::new().contains(&function_name)
                        || php_std::introspection::function_exists(registry, &function_name),
                )))
            }
            "get_defined_functions" => {
                if values.len() > 1 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_SYMBOL_ARITY: get_defined_functions expects zero or one argument",
                    );
                }
                self.call_get_defined_functions_builtin(compiled, output, state)
            }
            "get_defined_constants" => {
                if values.len() > 1 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_SYMBOL_ARITY: get_defined_constants expects zero or one argument",
                    );
                }
                let categorize = values
                    .first()
                    .is_some_and(|value| to_bool(value).unwrap_or(false));
                self.call_get_defined_constants_builtin(compiled, output, state, categorize)
            }
            "get_defined_vars" => {
                self.call_get_defined_vars_builtin(compiled, values, output, stack)
            }
            "compact" => self.call_compact_builtin(compiled, values, output, stack),
            "class_exists" | "interface_exists" | "trait_exists" | "enum_exists" => {
                let Some(class_name) = values.first() else {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!("E_PHP_VM_AUTOLOAD_ARITY: {name} expects class name"),
                    );
                };
                let class_name = match to_string(class_name) {
                    Ok(name) => name.to_string_lossy(),
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                let autoload = values
                    .get(1)
                    .is_none_or(|value| to_bool(value).unwrap_or(true));
                let lookup_kind = match name {
                    "interface_exists" => AutoloadClassLookupKind::Interface,
                    "enum_exists" => AutoloadClassLookupKind::Enum,
                    "trait_exists" => AutoloadClassLookupKind::Trait,
                    _ => AutoloadClassLookupKind::Class,
                };
                let exists = match self.class_like_exists_with_autoload_cache(
                    compiled,
                    &class_name,
                    lookup_kind,
                    autoload,
                    call_site,
                    output,
                    stack,
                    state,
                ) {
                    Ok(exists) => exists,
                    Err(result) => return result,
                };
                VmResult::success_no_output(Some(Value::Bool(exists)))
            }
            "class_alias" => self.call_class_alias_builtin(
                compiled, values, call_site, call_span, output, stack, state,
            ),
            "method_exists" => {
                self.call_method_exists_builtin(compiled, values, output, stack, state)
            }
            "property_exists" => {
                self.call_property_exists_builtin(compiled, values, output, stack, state)
            }
            "get_object_vars" => self.call_get_object_vars_builtin(compiled, values, output, stack),
            "get_mangled_object_vars" => {
                self.call_get_mangled_object_vars_builtin(compiled, values, output, stack)
            }
            "get_class_methods" => {
                self.call_get_class_methods_builtin(compiled, values, output, stack, state)
            }
            "get_class_vars" => {
                self.call_get_class_vars_builtin(compiled, values, output, stack, state)
            }
            "call_user_func" => {
                self.call_user_func_builtin(compiled, values, call_span, output, stack, state)
            }
            "call_user_func_array" => {
                self.call_user_func_array_builtin(compiled, values, call_span, output, stack, state)
            }
            "forward_static_call" => {
                self.call_forward_static_call_builtin(compiled, values, output, stack, state)
            }
            "func_get_args"
            | "func_num_args"
            | "func_get_arg"
            | "debug_backtrace"
            | "debug_print_backtrace" => {
                self.call_function_context_builtin(compiled, name, values, output, stack)
            }
            "get_called_class" => {
                self.call_get_called_class_builtin(compiled, values, output, stack, state)
            }
            "is_subclass_of" => {
                self.call_is_subclass_of_builtin(compiled, values, output, stack, state)
            }
            "is_a" => self.call_is_a_builtin(compiled, values, output, stack, state),
            "get_class" => self.call_get_class_builtin(compiled, values, output, stack, state),
            "get_parent_class" => {
                self.call_get_parent_class_builtin(compiled, values, output, stack, state)
            }
            "class_parents" => {
                self.call_class_parents_builtin(compiled, values, call_span, output, stack, state)
            }
            "class_implements" => self
                .call_class_implements_builtin(compiled, values, call_span, output, stack, state),
            "get_declared_classes" | "get_declared_interfaces" | "get_declared_traits" => {
                self.call_get_declared_builtin(compiled, name, output, state)
            }
            "get_loaded_extensions" => {
                let registry = php_std::ExtensionRegistry::standard_library();
                VmResult::success_no_output(Some(
                    php_std::introspection::get_loaded_extensions_value(registry),
                ))
            }
            "get_extension_funcs" => {
                if values.len() != 1 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_SYMBOL_ARITY: get_extension_funcs expects one argument",
                    );
                }
                let extension_name = match to_string(&values[0]) {
                    Ok(name) => name.to_string_lossy(),
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                let registry = php_std::ExtensionRegistry::standard_library();
                VmResult::success_no_output(Some(
                    php_std::introspection::get_extension_funcs_value(registry, &extension_name),
                ))
            }
            "get_included_files" | "get_required_files" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!("E_PHP_VM_SYMBOL_ARITY: {name} expects no arguments"),
                    );
                }
                VmResult::success_no_output(Some(Self::get_included_files_value(
                    compiled, stack, state,
                )))
            }
            "phpversion" => {
                if values.len() > 1 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_SYMBOL_ARITY: phpversion expects zero or one argument",
                    );
                }
                let registry = php_std::ExtensionRegistry::standard_library();
                let value = match values.first().map(effective_value) {
                    None | Some(Value::Null) => Value::string(php_std::constants::PHP_VERSION),
                    Some(value) => {
                        let extension_name = match to_string(&value) {
                            Ok(name) => name.to_string_lossy(),
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if php_std::introspection::extension_loaded(registry, &extension_name) {
                            Value::string(php_std::constants::PHP_VERSION)
                        } else {
                            Value::Bool(false)
                        }
                    }
                };
                VmResult::success_no_output(Some(value))
            }
            "zend_version" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_SYMBOL_ARITY: zend_version expects no arguments",
                    );
                }
                VmResult::success_no_output(Some(Value::string(php_std::constants::ZEND_VERSION)))
            }
            _ => self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_AUTOLOAD_BUILTIN: {name}"),
            ),
        }
    }

    pub(super) fn autoload_callable_class(
        &self,
        compiled: &CompiledUnit,
        value: &Value,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        match effective_value(value) {
            Value::String(name) => {
                let name = name.to_string_lossy();
                if let Some((class_name, _)) = name.split_once("::")
                    && lookup_class_in_state(compiled, state, class_name).is_none()
                {
                    self.autoload_class(compiled, class_name, output, stack, state, None)?;
                }
            }
            Value::Array(array) => {
                let Some(target) = array.get(&ArrayKey::Int(0)) else {
                    return Ok(());
                };
                let Some(method) = array.get(&ArrayKey::Int(1)) else {
                    return Ok(());
                };
                if to_string(method).is_err() {
                    return Ok(());
                }
                if object_from_value(target).is_none()
                    && let Ok(class_name) = to_string(target).map(|name| name.to_string_lossy())
                    && lookup_class_in_state(compiled, state, &class_name).is_none()
                {
                    self.autoload_class(compiled, &class_name, output, stack, state, None)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn call_method_exists_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if values.len() != 2 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_ARITY: method_exists expects two arguments",
            );
        }
        let class_name = match class_name_from_object_or_string(&values[0]) {
            Ok(name) => name,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let method = match to_string(&values[1]) {
            Ok(name) => name.to_string_lossy(),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        if lookup_class_in_state(compiled, state, &class_name).is_none()
            && let Err(result) =
                self.autoload_class(compiled, &class_name, output, stack, state, None)
        {
            return result;
        }
        let exists = lookup_method_in_state(compiled, state, &class_name, &method)
            .map(|value| value.is_some())
            .unwrap_or(false)
            || internal_class_has_method(&class_name, &method);
        VmResult::success_no_output(Some(Value::Bool(exists)))
    }

    pub(super) fn call_property_exists_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if values.len() != 2 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_ARITY: property_exists expects two arguments",
            );
        }
        let object = object_from_value(&values[0]);
        let class_name = match object.as_ref() {
            Some(object) => object.class_name(),
            None => match to_string(&values[0]) {
                Ok(name) => name.to_string_lossy(),
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            },
        };
        let property = match to_string(&values[1]) {
            Ok(name) => name.to_string_lossy(),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        if let Some(object) = object
            && object
                .properties_snapshot()
                .into_iter()
                .any(|(name, _)| name == property)
        {
            return VmResult::success_no_output(Some(Value::Bool(true)));
        }
        if lookup_class_in_state(compiled, state, &class_name).is_none()
            && let Err(result) =
                self.autoload_class(compiled, &class_name, output, stack, state, None)
        {
            return result;
        }
        let exists = lookup_property_in_state(compiled, state, &class_name, &property)
            .map(|value| value.is_some())
            .unwrap_or(false);
        VmResult::success_no_output(Some(Value::Bool(exists)))
    }

    pub(super) fn call_get_object_vars_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
    ) -> VmResult {
        if values.len() != 1 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_ARITY: get_object_vars expects one object argument",
            );
        }
        let Some(object) = object_from_value(&values[0]) else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_TYPE: get_object_vars expects object",
            );
        };
        VmResult::success_no_output(Some(Value::Array(object_vars_array(
            compiled, stack, &object, false,
        ))))
    }

    pub(super) fn call_get_mangled_object_vars_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
    ) -> VmResult {
        if values.len() != 1 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_ARITY: get_mangled_object_vars expects one object argument",
            );
        }
        let Some(object) = object_from_value(&values[0]) else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_TYPE: get_mangled_object_vars expects object",
            );
        };
        VmResult::success_no_output(Some(Value::Array(object_vars_array(
            compiled, stack, &object, true,
        ))))
    }

    pub(super) fn call_get_class_methods_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if values.len() != 1 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_ARITY: get_class_methods expects one argument",
            );
        }
        let class_name = match class_name_from_object_or_string(&values[0]) {
            Ok(name) => name,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        if lookup_class_in_state(compiled, state, &class_name).is_none()
            && let Err(result) =
                self.autoload_class(compiled, &class_name, output, stack, state, None)
        {
            return result;
        }
        let Some(class) = lookup_class_in_state(compiled, state, &class_name) else {
            return VmResult::success_no_output(Some(Value::Bool(false)));
        };
        let mut array = PhpArray::new();
        for name in visible_class_methods(compiled, stack, state, &class) {
            array.append(Value::string(name));
        }
        VmResult::success_no_output(Some(Value::Array(array)))
    }

    pub(super) fn call_get_class_vars_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if values.len() != 1 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_ARITY: get_class_vars expects one class argument",
            );
        }
        let class_name = match to_string(&values[0]) {
            Ok(name) => name.to_string_lossy(),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        if lookup_class_in_state(compiled, state, &class_name).is_none()
            && let Err(result) =
                self.autoload_class(compiled, &class_name, output, stack, state, None)
        {
            return result;
        }
        let Some(class) = lookup_class_in_state(compiled, state, &class_name) else {
            return VmResult::success_no_output(Some(Value::Bool(false)));
        };
        VmResult::success_no_output(Some(Value::Array(visible_class_vars(
            compiled, stack, state, &class,
        ))))
    }

    pub(super) fn call_get_called_class_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &ExecutionState,
    ) -> VmResult {
        if !values.is_empty() {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_CALL_CONTEXT_ARITY: get_called_class expects no arguments",
            );
        }
        let Some(called_class) = current_called_class(compiled, stack) else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_CALL_CONTEXT: get_called_class is not available outside class scope",
            );
        };
        let display_name = lookup_class_in_state(compiled, state, &called_class)
            .map(|class| class.display_name.clone())
            .unwrap_or(called_class);
        VmResult::success_no_output(Some(Value::string(display_name)))
    }

    pub(super) fn call_is_subclass_of_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if !(2..=3).contains(&values.len()) {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_ARITY: is_subclass_of expects two or three arguments",
            );
        }
        let allow_string = values
            .get(2)
            .is_none_or(|value| to_bool(value).unwrap_or(true));
        let class_name = match object_from_value(&values[0]) {
            Some(object) => object.class_name(),
            None if allow_string => match to_string(&values[0]) {
                Ok(name) => name.to_string_lossy(),
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            },
            None => return VmResult::success_no_output(Some(Value::Bool(false))),
        };
        let target_name = match to_string(&values[1]) {
            Ok(name) => name.to_string_lossy(),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        for candidate in [&class_name, &target_name] {
            if lookup_class_in_state(compiled, state, candidate).is_none()
                && let Err(result) =
                    self.autoload_class(compiled, candidate, output, stack, state, None)
            {
                return result;
            }
        }
        let exists = class_is_subclass_of_in_state(compiled, state, &class_name, &target_name)
            .unwrap_or(false);
        VmResult::success_no_output(Some(Value::Bool(exists)))
    }

    pub(super) fn call_is_a_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if !(2..=3).contains(&values.len()) {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_ARITY: is_a expects two or three arguments",
            );
        }
        let allow_string = values
            .get(2)
            .is_some_and(|value| to_bool(value).unwrap_or(false));
        let class_name = match class_name_for_is_a_subject(&values[0], allow_string) {
            Ok(Some(name)) => name,
            Ok(None) => return VmResult::success_no_output(Some(Value::Bool(false))),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let target_name = match to_string(&values[1]) {
            Ok(name) => name.to_string_lossy(),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        for candidate in [&class_name, &target_name] {
            if lookup_class_in_state(compiled, state, candidate).is_none()
                && let Err(result) =
                    self.autoload_class(compiled, candidate, output, stack, state, None)
            {
                return result;
            }
        }
        let exists =
            class_is_a_in_state(compiled, state, &class_name, &target_name).unwrap_or(false);
        VmResult::success_no_output(Some(Value::Bool(exists)))
    }

    pub(super) fn call_get_class_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &ExecutionState,
    ) -> VmResult {
        if values.len() != 1 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_ARITY: get_class expects one object argument",
            );
        }
        let Some(object) = object_from_value(&values[0]) else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_TYPE: get_class expects object",
            );
        };
        let class_name = object.class_name();
        let display_name = lookup_class_in_state(compiled, state, &class_name)
            .map(|class| class.display_name.clone())
            .unwrap_or_else(|| object.display_name());
        VmResult::success_no_output(Some(Value::string(display_name)))
    }

    pub(super) fn call_get_parent_class_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if values.len() != 1 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_ARITY: get_parent_class expects one argument",
            );
        }
        let class_name = match class_name_from_object_or_string(&values[0]) {
            Ok(name) => name,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        if lookup_class_in_state(compiled, state, &class_name).is_none()
            && let Err(result) =
                self.autoload_class(compiled, &class_name, output, stack, state, None)
        {
            return result;
        }
        let parent = lookup_class_in_state(compiled, state, &class_name)
            .and_then(|class| class.parent.clone())
            .and_then(|parent| {
                lookup_class_in_state(compiled, state, &parent)
                    .map(|class| class.display_name.clone())
                    .or(Some(parent))
            })
            .map(Value::string)
            .unwrap_or(Value::Bool(false));
        VmResult::success_no_output(Some(parent))
    }

    pub(super) fn call_class_parents_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if values.is_empty() || values.len() > 2 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_ARITY: class_parents expects one or two arguments",
            );
        }
        let class_name = match class_name_from_object_or_string(&values[0]) {
            Ok(name) => name,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let autoload = values
            .get(1)
            .is_none_or(|value| to_bool(value).unwrap_or(true));
        let should_autoload =
            autoload && lookup_class_in_state(compiled, state, &class_name).is_none();
        if should_autoload
            && let Err(result) =
                self.autoload_class(compiled, &class_name, output, stack, state, None)
        {
            return result;
        }
        let Some(class) = lookup_class_in_state(compiled, state, &class_name) else {
            if let Err(result) = self.emit_class_introspection_missing_warning(
                compiled,
                "class_parents",
                &class_name,
                should_autoload,
                call_span,
                output,
                stack,
                state,
            ) {
                return result;
            }
            return VmResult::success_no_output(Some(Value::Bool(false)));
        };
        let mut array = PhpArray::new();
        let mut parent = class.parent.clone();
        let mut seen = Vec::new();
        while let Some(parent_name) = parent {
            let normalized = normalize_class_name(&parent_name);
            if seen.iter().any(|name| name == &normalized) {
                break;
            }
            seen.push(normalized.clone());
            let display = lookup_class_in_state(compiled, state, &normalized)
                .map(|class| class.display_name.clone())
                .unwrap_or(parent_name);
            array.insert(string_key(&display), Value::string(display.clone()));
            parent = lookup_class_in_state(compiled, state, &normalized)
                .and_then(|class| class.parent.clone());
        }
        VmResult::success_no_output(Some(Value::Array(array)))
    }

    pub(super) fn call_class_implements_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if values.is_empty() || values.len() > 2 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_ARITY: class_implements expects one or two arguments",
            );
        }
        let class_name = match class_name_from_object_or_string(&values[0]) {
            Ok(name) => name,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let autoload = values
            .get(1)
            .is_none_or(|value| to_bool(value).unwrap_or(true));
        let should_autoload =
            autoload && lookup_class_in_state(compiled, state, &class_name).is_none();
        if should_autoload
            && let Err(result) =
                self.autoload_class(compiled, &class_name, output, stack, state, None)
        {
            return result;
        }
        let Some(class) = lookup_class_in_state(compiled, state, &class_name) else {
            if let Err(result) = self.emit_class_introspection_missing_warning(
                compiled,
                "class_implements",
                &class_name,
                should_autoload,
                call_span,
                output,
                stack,
                state,
            ) {
                return result;
            }
            return VmResult::success_no_output(Some(Value::Bool(false)));
        };
        let mut array = PhpArray::new();
        let mut interfaces = Vec::new();
        collect_class_interface_display_names(compiled, state, &class.name, &mut interfaces);
        for (_, display) in interfaces {
            array.insert(string_key(&display), Value::string(display));
        }
        VmResult::success_no_output(Some(Value::Array(array)))
    }

    pub(super) fn emit_class_introspection_missing_warning(
        &self,
        compiled: &CompiledUnit,
        function_name: &str,
        class_name: &str,
        autoload_attempted: bool,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        let suffix = if autoload_attempted {
            "does not exist and could not be loaded"
        } else {
            "does not exist"
        };
        let message = format!("{function_name}(): Class {class_name} {suffix}");
        let source_span = call_span
            .or_else(|| stack.current().and_then(|frame| frame.call_span))
            .map(|span| runtime_source_span(compiled, span))
            .unwrap_or_default();
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_VM_SYMBOL_CLASS_WARNING",
            RuntimeSeverity::Warning,
            message,
            source_span,
            stack_trace(compiled, stack),
            Some(php_runtime::PhpReferenceClassification::Warning),
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::PHP_E_WARNING,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::PHP_E_WARNING) {
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::PhpDiagnosticChannel::Warning,
                php_runtime::PHP_E_WARNING,
            );
            state.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    pub(super) fn emit_class_alias_warning(
        &self,
        compiled: &CompiledUnit,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        message: String,
    ) -> Result<(), VmResult> {
        let source_span = call_span
            .or_else(|| stack.current().and_then(|frame| frame.call_span))
            .map(|span| runtime_source_span(compiled, span))
            .unwrap_or_default();
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_VM_CLASS_ALIAS_WARNING",
            RuntimeSeverity::Warning,
            message,
            source_span,
            stack_trace(compiled, stack),
            Some(php_runtime::PhpReferenceClassification::Warning),
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::PHP_E_WARNING,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::PHP_E_WARNING) {
            Self::record_last_error(state, php_runtime::PHP_E_WARNING, &diagnostic);
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::PhpDiagnosticChannel::Warning,
                php_runtime::PHP_E_WARNING,
            );
            state.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    pub(super) fn emit_spl_autoload_call_unregister_deprecation(
        &self,
        compiled: &CompiledUnit,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        let source_span = call_span
            .or_else(|| stack.current().and_then(|frame| frame.call_span))
            .map(|span| runtime_source_span(compiled, span))
            .unwrap_or_default();
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_VM_SPL_AUTOLOAD_CALL_UNREGISTER_DEPRECATED",
            RuntimeSeverity::Deprecation,
            "spl_autoload_unregister(): Using spl_autoload_call() as a callback for spl_autoload_unregister() is deprecated, to remove all registered autoloaders, call spl_autoload_unregister() for all values returned from spl_autoload_functions()"
                .to_owned(),
            source_span,
            stack_trace(compiled, stack),
            None,
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::PHP_E_DEPRECATED,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::PHP_E_DEPRECATED) {
            Self::record_last_error(state, php_runtime::PHP_E_DEPRECATED, &diagnostic);
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::PhpDiagnosticChannel::Deprecated,
                php_runtime::PHP_E_DEPRECATED,
            );
            state.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    pub(super) fn emit_predefined_constant_deprecation(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
        instruction_span: php_ir::IrSpan,
        constant: &php_std::ConstantDescriptor,
    ) -> Result<(), VmResult> {
        let Some(deprecation) = constant.deprecation() else {
            return Ok(());
        };
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_VM_DEPRECATED_CONSTANT",
            RuntimeSeverity::Deprecation,
            deprecation.message().to_owned(),
            current_instruction_diagnostic_span(compiled, state, instruction_span),
            stack_trace(compiled, stack),
            Some(php_runtime::PhpReferenceClassification::Deprecation),
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::PHP_E_DEPRECATED,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::PHP_E_DEPRECATED) {
            Self::record_last_error(state, php_runtime::PHP_E_DEPRECATED, &diagnostic);
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::PhpDiagnosticChannel::Deprecated,
                php_runtime::PHP_E_DEPRECATED,
            );
            diagnostics.push(diagnostic);
        }
        Ok(())
    }

    pub(super) fn emit_spl_autoload_register_do_throw_notice(
        &self,
        compiled: &CompiledUnit,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        let source_span = call_span
            .or_else(|| stack.current().and_then(|frame| frame.call_span))
            .map(|span| runtime_source_span(compiled, span))
            .unwrap_or_default();
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_VM_SPL_AUTOLOAD_REGISTER_DO_THROW_NOTICE",
            RuntimeSeverity::Notice,
            "spl_autoload_register(): Argument #2 ($do_throw) has been ignored, spl_autoload_register() will always throw"
                .to_owned(),
            source_span,
            stack_trace(compiled, stack),
            None,
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::PHP_E_NOTICE,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::PHP_E_NOTICE) {
            Self::record_last_error(state, php_runtime::PHP_E_NOTICE, &diagnostic);
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::PhpDiagnosticChannel::Notice,
                php_runtime::PHP_E_NOTICE,
            );
            state.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    pub(super) fn emit_define_duplicate_warning(
        &self,
        compiled: &CompiledUnit,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
        constant_name: &str,
    ) -> Result<(), VmResult> {
        let source_span = call_span
            .or_else(|| stack.current().and_then(|frame| frame.call_span))
            .map(|span| runtime_source_span(compiled, span))
            .unwrap_or_default();
        self.emit_constant_already_defined_warning(
            compiled,
            source_span,
            output,
            stack,
            state,
            diagnostics,
            constant_name,
        )
    }

    pub(super) fn emit_constant_already_defined_warning(
        &self,
        compiled: &CompiledUnit,
        source_span: RuntimeSourceSpan,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
        constant_name: &str,
    ) -> Result<(), VmResult> {
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_VM_DEFINE_CONSTANT_ALREADY_DEFINED",
            RuntimeSeverity::Warning,
            format!("Constant {constant_name} already defined, this will be an error in PHP 9"),
            source_span,
            stack_trace(compiled, stack),
            None,
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::PHP_E_WARNING,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::PHP_E_WARNING) {
            Self::record_last_error(state, php_runtime::PHP_E_WARNING, &diagnostic);
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::PhpDiagnosticChannel::Warning,
                php_runtime::PHP_E_WARNING,
            );
            diagnostics.push(diagnostic);
        }
        Ok(())
    }

    pub(super) fn emit_duplicate_dynamic_constant_warnings(
        &self,
        compiled: &CompiledUnit,
        dynamic_unit: &CompiledUnit,
        load_kind: DeclarationLoadKind,
        eval_span: Option<IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<(), VmResult> {
        for entry in &dynamic_unit.unit().constant_table {
            if !dynamic_constant_declared(compiled, state, &entry.name) {
                continue;
            }
            let source_span = if load_kind == DeclarationLoadKind::Eval {
                eval_span
                    .map(|span| eval_diagnostic_source_span(compiled, span))
                    .unwrap_or_else(|| runtime_source_span(dynamic_unit, entry.span))
            } else {
                runtime_source_span(dynamic_unit, entry.span)
            };
            self.emit_constant_already_defined_warning(
                compiled,
                source_span,
                output,
                stack,
                state,
                diagnostics,
                &entry.name,
            )?;
        }
        Ok(())
    }

    pub(super) fn call_get_declared_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        _output: &mut OutputBuffer,
        state: &ExecutionState,
    ) -> VmResult {
        let classes = declared_classes_in_state(compiled, state);
        let values = classes
            .into_iter()
            .filter(|class| match name {
                "get_declared_interfaces" => class.flags.is_interface,
                "get_declared_traits" => class.flags.is_trait,
                _ => !class.flags.is_interface && !class.flags.is_trait,
            })
            .map(|class| Value::string(class.display_name))
            .collect();
        VmResult::success_no_output(Some(Value::Array(PhpArray::from_packed(values))))
    }

    pub(super) fn call_get_defined_functions_builtin(
        &self,
        compiled: &CompiledUnit,
        _output: &mut OutputBuffer,
        state: &ExecutionState,
    ) -> VmResult {
        let mut user_functions = BTreeSet::new();
        for entry in compiled.function_table() {
            let function = compiled.unit().functions.get(entry.function.index());
            if function.is_some_and(|function| function.flags.is_closure) {
                continue;
            }
            user_functions.insert(entry.name.clone());
        }
        for entry in &state.dynamic_functions {
            user_functions.insert(entry.name.clone());
        }
        let internal_functions = php_std::ExtensionRegistry::standard_library()
            .enabled_php_functions()
            .into_iter()
            .map(|function| function.name().to_owned())
            .collect::<BTreeSet<_>>();

        let mut array = PhpArray::new();
        array.insert(
            string_key("internal"),
            Value::Array(defined_symbol_names_array(&internal_functions)),
        );
        array.insert(
            string_key("user"),
            Value::Array(defined_symbol_names_array(&user_functions)),
        );
        VmResult::success_no_output(Some(Value::Array(array)))
    }

    pub(super) fn call_get_defined_constants_builtin(
        &self,
        compiled: &CompiledUnit,
        _output: &mut OutputBuffer,
        state: &ExecutionState,
        categorize: bool,
    ) -> VmResult {
        let mut standard = PhpArray::new();
        for constant in php_std::ExtensionRegistry::standard_library().enabled_constants() {
            if let Some(value) = predefined_constant_value_for_state(state, constant.name()) {
                standard.insert(string_key(constant.name()), value);
            }
        }

        let mut user = PhpArray::new();
        for entry in compiled.constant_table() {
            if let Ok(value) = constant_value(compiled.unit(), entry.value) {
                user.insert(string_key(&entry.name), value);
            }
        }
        for entry in &state.dynamic_constants {
            if let Some(owner) = state.dynamic_units.get(entry.unit_index)
                && let Ok(value) = constant_value(owner.unit(), entry.value)
            {
                user.insert(string_key(&entry.name), value);
            }
        }
        for (name, value) in &state.user_constants {
            user.insert(string_key(name), value.clone());
        }

        let value = if categorize {
            let mut array = PhpArray::new();
            array.insert(string_key("Core"), Value::Array(standard));
            array.insert(string_key("user"), Value::Array(user));
            Value::Array(array)
        } else {
            for (key, value) in user.iter() {
                standard.insert(key.clone(), value.clone());
            }
            Value::Array(standard)
        };
        VmResult::success_no_output(Some(value))
    }

    pub(super) fn call_class_alias_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        call_site: Option<(u64, FunctionId, BlockId, InstrId)>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if values.len() < 2 || values.len() > 3 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_CLASS_ALIAS_ARITY: class_alias expects two or three arguments",
            );
        }
        let source_name = match to_string(&values[0]) {
            Ok(name) => name.to_string_lossy(),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let alias_name = match to_string(&values[1]) {
            Ok(name) => name.to_string_lossy(),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let autoload = values
            .get(2)
            .is_none_or(|value| to_bool(value).unwrap_or(true));
        let exists = match self.class_like_exists_with_autoload_cache(
            compiled,
            &source_name,
            AutoloadClassLookupKind::ClassLike,
            autoload,
            call_site,
            output,
            stack,
            state,
        ) {
            Ok(exists) => exists,
            Err(result) => return result,
        };
        if !exists {
            if let Err(result) = self.emit_class_alias_warning(
                compiled,
                call_span,
                output,
                stack,
                state,
                format!("Class \"{source_name}\" not found"),
            ) {
                return result;
            }
            return VmResult::success_no_output(Some(Value::Bool(false)));
        }
        if lookup_class_in_state(compiled, state, &alias_name).is_some()
            || php_std::ExtensionRegistry::standard_library()
                .enabled_class(&alias_name)
                .is_some()
        {
            if let Err(result) = self.emit_class_alias_warning(
                compiled,
                call_span,
                output,
                stack,
                state,
                format!("Cannot redeclare class {}", display_class_name(&alias_name)),
            ) {
                return result;
            }
            return VmResult::success_no_output(Some(Value::Bool(false)));
        }

        let Some(class) = lookup_class_in_state(compiled, state, &source_name) else {
            if let Err(result) = self.emit_class_alias_warning(
                compiled,
                call_span,
                output,
                stack,
                state,
                format!("Class \"{source_name}\" not found"),
            ) {
                return result;
            }
            return VmResult::success_no_output(Some(Value::Bool(false)));
        };
        let unit_index = dynamic_class_owner_index_in_state(state, &class.name)
            .unwrap_or_else(|| dynamic_or_retain_unit_index(state, compiled));
        state.push_dynamic_class(DynamicClassEntry {
            lookup_name: normalize_class_name(&alias_name),
            class,
            unit_index,
            origin: declaration_origin(
                compiled,
                call_span.unwrap_or_default(),
                &alias_name,
                DeclarationKind::ClassLike,
                DeclarationLoadKind::Conditional,
            ),
        });
        state.bump_class_table_epoch();
        VmResult::success_no_output(Some(Value::Bool(true)))
    }

    pub(super) fn class_like_exists_with_autoload_cache(
        &self,
        compiled: &CompiledUnit,
        class_name: &str,
        kind: AutoloadClassLookupKind,
        autoload: bool,
        call_site: Option<(u64, FunctionId, BlockId, InstrId)>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<bool, VmResult> {
        self.class_like_exists_with_autoload_cache_at_slot(
            compiled, class_name, kind, autoload, call_site, None, output, stack, state,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn class_like_exists_with_autoload_cache_at_slot(
        &self,
        compiled: &CompiledUnit,
        class_name: &str,
        kind: AutoloadClassLookupKind,
        autoload: bool,
        call_site: Option<(u64, FunctionId, BlockId, InstrId)>,
        cache_id: Option<InlineCacheId>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<bool, VmResult> {
        let request = AutoloadClassLookupCacheKey {
            kind,
            normalized_name: normalize_class_name(class_name),
            autoload_enabled: autoload,
            autoload_stack_depth: state.autoload_stack.len(),
            include_path_config: state.ini.get("include_path").unwrap_or(".").to_owned(),
            composer_map_fingerprint: self.composer_map_fingerprint(state),
        };
        let epochs = state.autoload_class_lookup_epochs();
        if let Some((unit_key, function, block, instruction)) = call_site {
            self.observe_autoload_class_inline_cache(
                cache_id,
                unit_key,
                function,
                block,
                instruction,
            );
            if let Some(target) = self.lookup_autoload_class_inline_cache(
                cache_id,
                unit_key,
                function,
                block,
                instruction,
                &request,
                epochs,
            ) {
                match target {
                    AutoloadClassLookupCacheTarget::Positive { .. } => {
                        if class_like_exists_direct(compiled, state, class_name, kind) {
                            return Ok(true);
                        }
                        self.record_counter_invalidation_by_reason(
                            "autoload_positive_target_missing",
                        );
                        self.invalidate_autoload_class_inline_cache(
                            cache_id,
                            unit_key,
                            function,
                            block,
                            instruction,
                        );
                    }
                    AutoloadClassLookupCacheTarget::Negative => {
                        self.record_counter_negative_lookup_hit();
                        return Ok(false);
                    }
                }
            }
        }

        if class_like_exists_direct(compiled, state, class_name, kind) {
            if let Some((unit_key, function, block, instruction)) = call_site {
                self.install_autoload_class_inline_cache(
                    cache_id,
                    unit_key,
                    function,
                    block,
                    instruction,
                    request,
                    epochs,
                    AutoloadClassLookupCacheTarget::Positive {
                        display_name: class_name.to_owned(),
                    },
                );
            }
            return Ok(true);
        }

        let may_autoload_with_side_effects =
            autoload && !state.autoload_registry.callbacks().is_empty();
        if autoload {
            let trace_origin = autoload_trace_origin_from_call_site(
                compiled,
                kind.exists_function_name(),
                call_site,
            );
            self.autoload_class(compiled, class_name, output, stack, state, trace_origin)?;
        }

        let exists = class_like_exists_direct(compiled, state, class_name, kind);
        if let Some((unit_key, function, block, instruction)) = call_site {
            let post_epochs = state.autoload_class_lookup_epochs();
            if exists {
                self.install_autoload_class_inline_cache(
                    cache_id,
                    unit_key,
                    function,
                    block,
                    instruction,
                    request,
                    post_epochs,
                    AutoloadClassLookupCacheTarget::Positive {
                        display_name: class_name.to_owned(),
                    },
                );
            } else if !may_autoload_with_side_effects {
                self.install_autoload_class_inline_cache(
                    cache_id,
                    unit_key,
                    function,
                    block,
                    instruction,
                    request,
                    post_epochs,
                    AutoloadClassLookupCacheTarget::Negative,
                );
            }
        }
        Ok(exists)
    }

    pub(super) fn autoload_static_class_if_missing(
        &self,
        compiled: &CompiledUnit,
        class_name: &str,
        span: IrSpan,
        call_site: Option<(u64, FunctionId, BlockId, InstrId)>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        self.autoload_static_class_if_missing_at_slot(
            compiled, class_name, span, call_site, None, output, stack, state,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn autoload_static_class_if_missing_at_slot(
        &self,
        compiled: &CompiledUnit,
        class_name: &str,
        span: IrSpan,
        call_site: Option<(u64, FunctionId, BlockId, InstrId)>,
        cache_id: Option<InlineCacheId>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        if is_special_static_class_name(class_name)
            || class_like_exists_direct(
                compiled,
                state,
                class_name,
                AutoloadClassLookupKind::ClassLike,
            )
        {
            return Ok(());
        }

        let lookup_name = static_class_autoload_name(compiled, class_name, span);
        self.class_like_exists_with_autoload_cache_at_slot(
            compiled,
            &lookup_name,
            AutoloadClassLookupKind::ClassLike,
            true,
            call_site,
            cache_id,
            output,
            stack,
            state,
        )?;
        Ok(())
    }

    pub(super) fn validate_runtime_class_dependencies(
        &self,
        compiled: &CompiledUnit,
        declared_unit: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Option<VmResult> {
        match self.validate_runtime_class_dependencies_matching(
            compiled,
            declared_unit,
            output,
            stack,
            state,
            |_| true,
        )? {
            ClassDependencyValidationFailure::Throwable(throwable) => {
                state.pending_trace = Some(capture_backtrace_string(compiled, stack));
                Some(self.handle_uncaught_exception(compiled, output, stack, state, throwable))
            }
            ClassDependencyValidationFailure::Result(result) => Some(*result),
        }
    }

    pub(super) fn validate_runtime_class_dependencies_in_try(
        &self,
        compiled: &CompiledUnit,
        function: &php_ir::function::IrFunction,
        catch: BlockId,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        try_span: IrSpan,
    ) -> Option<ClassDependencyValidationFailure> {
        let catch_start = function
            .blocks
            .get(catch.index())
            .and_then(block_first_span)
            .map_or(try_span.end, |span| span.start);
        self.validate_runtime_class_dependencies_matching(
            compiled,
            compiled,
            output,
            stack,
            state,
            |class| {
                class.span != IrSpan::default()
                    && class.span.file == try_span.file
                    && class.span.start >= try_span.start
                    && class.span.end <= catch_start
            },
        )
    }

    pub(super) fn validate_runtime_class_dependencies_matching(
        &self,
        compiled: &CompiledUnit,
        declared_unit: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        mut matches_class: impl FnMut(&php_ir::module::ClassEntry) -> bool,
    ) -> Option<ClassDependencyValidationFailure> {
        for class in &declared_unit.unit().classes {
            if should_defer_class_dependency_validation(class) {
                continue;
            }
            if !matches_class(class) {
                continue;
            }
            let normalized_class = normalize_class_name(&class.name);
            if state
                .validated_class_dependencies
                .contains(&normalized_class)
            {
                continue;
            }

            if let Some(parent) = class.parent.as_deref() {
                let display = class_dependency_display_name(declared_unit, class, parent);
                match self.class_like_exists_with_autoload_cache(
                    compiled,
                    &display,
                    AutoloadClassLookupKind::Class,
                    true,
                    None,
                    output,
                    stack,
                    state,
                ) {
                    Ok(true) => {}
                    Ok(false) => {
                        return Some(
                            match self.class_dependency_not_found_throwable(
                                declared_unit,
                                class,
                                &display,
                                "Class",
                                state,
                            ) {
                                Ok(throwable) => {
                                    ClassDependencyValidationFailure::Throwable(throwable)
                                }
                                Err(result) => {
                                    ClassDependencyValidationFailure::Result(Box::new(result))
                                }
                            },
                        );
                    }
                    Err(result) => {
                        return Some(ClassDependencyValidationFailure::Result(Box::new(result)));
                    }
                }
            }

            for interface in &class.interfaces {
                if normalize_class_name(interface) == "datetimeinterface"
                    && !class_extends_datetime_implementation(declared_unit, class)
                {
                    return Some(ClassDependencyValidationFailure::Result(Box::new(
                        self.datetime_interface_user_implementation_fatal(
                            declared_unit,
                            class,
                            output,
                            stack,
                        ),
                    )));
                }
                let display = class_dependency_display_name(declared_unit, class, interface);
                match self.class_like_exists_with_autoload_cache(
                    compiled,
                    &display,
                    AutoloadClassLookupKind::Interface,
                    true,
                    None,
                    output,
                    stack,
                    state,
                ) {
                    Ok(true) => {}
                    Ok(false) => {
                        return Some(
                            match self.class_dependency_not_found_throwable(
                                declared_unit,
                                class,
                                &display,
                                "Interface",
                                state,
                            ) {
                                Ok(throwable) => {
                                    ClassDependencyValidationFailure::Throwable(throwable)
                                }
                                Err(result) => {
                                    ClassDependencyValidationFailure::Result(Box::new(result))
                                }
                            },
                        );
                    }
                    Err(result) => {
                        return Some(ClassDependencyValidationFailure::Result(Box::new(result)));
                    }
                }
            }

            state.failed_class_declarations.remove(&normalized_class);
            state.validated_class_dependencies.insert(normalized_class);
        }
        None
    }

    pub(super) fn class_dependency_not_found_throwable(
        &self,
        compiled: &CompiledUnit,
        class: &php_ir::module::ClassEntry,
        dependency: &str,
        kind: &str,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
        state
            .failed_class_declarations
            .insert(normalize_class_name(&class.name));
        state.bump_class_table_epoch();
        let message = Value::string(format!("{kind} \"{dependency}\" not found").into_bytes());
        let throwable = match make_exception_object("Error", &message) {
            Ok(object) => Value::Object(object),
            Err(message) => {
                return Err(VmResult::runtime_error_with_diagnostic(
                    OutputBuffer::new(),
                    format!("E_PHP_VM_CLASS_DEPENDENCY_ERROR: {message}"),
                    RuntimeDiagnostic::new(
                        "E_PHP_VM_CLASS_DEPENDENCY_ERROR",
                        RuntimeSeverity::FatalError,
                        format!("E_PHP_VM_CLASS_DEPENDENCY_ERROR: {message}"),
                        RuntimeSourceSpan::default(),
                        Vec::new(),
                        None,
                    ),
                ));
            }
        };
        tag_throwable_location(&throwable, compiled, class.span);
        Ok(throwable)
    }

    pub(super) fn datetime_interface_user_implementation_fatal(
        &self,
        compiled: &CompiledUnit,
        class: &php_ir::module::ClassEntry,
        output: &mut OutputBuffer,
        stack: &CallStack,
    ) -> VmResult {
        let source_span = runtime_source_span(compiled, class.span);
        let location = php_runtime::PhpDiagnosticLocation::from_span(&source_span);
        let message = "DateTimeInterface can't be implemented by user classes";
        output.write_test_str(&format!(
            "\nFatal error: {message} in {} on line {}\n",
            location.file, location.line
        ));
        VmResult::runtime_error_with_diagnostic(
            output.clone(),
            format!("E_PHP_VM_DATETIMEINTERFACE_USER_IMPLEMENTATION: {message}"),
            RuntimeDiagnostic::new(
                "E_PHP_VM_DATETIMEINTERFACE_USER_IMPLEMENTATION",
                RuntimeSeverity::FatalError,
                format!("E_PHP_VM_DATETIMEINTERFACE_USER_IMPLEMENTATION: {message}"),
                source_span,
                stack_trace(compiled, stack),
                Some(php_runtime::PhpReferenceClassification::FatalError),
            ),
        )
    }

    pub(super) fn preflight_reflection_constructor(
        &self,
        compiled: &CompiledUnit,
        reflection_class: &str,
        values: &[Value],
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        let reflection_class = normalize_class_name(reflection_class);
        if !matches!(
            reflection_class.as_str(),
            "reflectionclass"
                | "reflectionmethod"
                | "reflectionproperty"
                | "reflectionclassconstant"
                | "reflectionenum"
                | "reflectionenumunitcase"
                | "reflectionenumbackedcase"
        ) {
            return Ok(());
        }
        let Some(class_name) = values.first().and_then(reflection_value_string) else {
            return Ok(());
        };
        let kind = match reflection_class.as_str() {
            "reflectionclass" | "reflectionmethod" | "reflectionclassconstant" => {
                AutoloadClassLookupKind::ClassLike
            }
            "reflectionenum" | "reflectionenumunitcase" | "reflectionenumbackedcase" => {
                AutoloadClassLookupKind::Enum
            }
            _ => AutoloadClassLookupKind::Class,
        };
        let exists = self.class_like_exists_with_autoload_cache(
            compiled,
            &class_name,
            kind,
            true,
            None,
            output,
            stack,
            state,
        )?;
        if exists {
            return Ok(());
        }
        Err(self.runtime_error(
            output,
            compiled,
            stack,
            format!("E_PHP_VM_REFLECTION_UNKNOWN_CLASS: Class \"{class_name}\" does not exist"),
        ))
    }

    pub(super) fn reflection_attribute_new_instance(
        &self,
        compiled: &CompiledUnit,
        attribute: &ObjectRef,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        call_span: php_ir::IrSpan,
    ) -> VmResult {
        let class_name = match reflection_object_string_property(attribute, "name") {
            Ok(name) => name,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let class = match lookup_class_in_state(compiled, state, &class_name) {
            Some(class) => class,
            None => {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!("E_PHP_VM_UNKNOWN_CLASS: Class \"{class_name}\" not found"),
                );
            }
        };
        let runtime_class = match runtime_class_entry(
            compiled,
            state,
            &class,
            &|value| self.constant_value(compiled.unit(), value),
            &|reference| class_constant_reference_value(compiled, state, reference),
            &|reference| named_constant_reference_value(compiled, state, reference),
        ) {
            Ok(class) => class,
            Err(error) => return self.runtime_error(output, compiled, stack, error.into_message()),
        };
        if let Err(message) = validate_object_mvp(&runtime_class) {
            return self.runtime_error(output, compiled, stack, message);
        }

        let args = match reflection_attribute_constructor_args(attribute) {
            Ok(args) => args,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let object = ObjectRef::new_with_display_name(&runtime_class, class.display_name.clone());
        let caller_scope = current_scope_class(compiled, stack);
        let constructor = match lookup_method_in_hierarchy(
            compiled,
            &class,
            "__construct",
            caller_scope.as_deref(),
        ) {
            Ok(constructor) => constructor,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        if let Some(constructor) = constructor {
            if let Err(message) = validate_constructor_callable_in_state_scope(
                compiled,
                state,
                caller_scope.as_deref(),
                constructor.class,
                constructor.method,
            ) {
                return self.runtime_error(output, compiled, stack, message);
            }
            let class_owner = class_owner_in_state(compiled, state, &constructor.class.name);
            let result = self.execute_function(
                &class_owner,
                constructor.method.function,
                FunctionCall::new(args, Vec::new())
                    .with_call_site_strict_types(compiled.unit().strict_types)
                    .with_call_span(call_span)
                    .with_this(object.clone())
                    .with_class_context(
                        constructor.class.name.clone(),
                        class.name.clone(),
                        constructor.class.name.clone(),
                    ),
                output,
                stack,
                state,
            );
            if !result.status.is_success() || result.fiber_suspension.is_some() {
                return result;
            }
            self.register_destructor_if_needed(compiled, &class, object.clone(), state);
            return VmResult::success_with_diagnostics_no_output(
                Some(Value::Object(object)),
                result.diagnostics,
            );
        }
        if !args.is_empty() {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_TOO_MANY_ARGS: constructor for class {class_name} does not accept arguments"
                ),
            );
        }
        self.register_destructor_if_needed(compiled, &class, object.clone(), state);
        VmResult::success_no_output(Some(Value::Object(object)))
    }

    pub(super) fn reflection_class_new_instance(
        &self,
        compiled: &CompiledUnit,
        reflection_class: &ObjectRef,
        method: &str,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        call_span: php_ir::IrSpan,
    ) -> VmResult {
        let class_name = match reflection_object_string_property(reflection_class, "class") {
            Ok(name) => name,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let args = match normalize_method_name(method).as_str() {
            "newinstance" => values.into_iter().map(CallArgument::positional).collect(),
            "newinstanceargs" => match reflection_new_instance_args(values) {
                Ok(args) => args,
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            },
            _ => {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                        reflection_class.class_name(),
                        method
                    ),
                );
            }
        };
        self.reflection_instantiate_class(
            compiled,
            &class_name,
            args,
            output,
            stack,
            state,
            call_span,
        )
    }

    pub(super) fn reflection_instantiate_class(
        &self,
        compiled: &CompiledUnit,
        class_name: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        call_span: php_ir::IrSpan,
    ) -> VmResult {
        let class = match lookup_class_in_state(compiled, state, class_name) {
            Some(class) => class,
            None => {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!("E_PHP_VM_UNKNOWN_CLASS: Class \"{class_name}\" not found"),
                );
            }
        };
        let class_owner = class_owner_in_state(compiled, state, &class.name);
        let runtime_class = match runtime_class_entry(
            &class_owner,
            state,
            &class,
            &|value| self.constant_value(class_owner.unit(), value),
            &|reference| class_constant_reference_value(&class_owner, state, reference),
            &|reference| named_constant_reference_value(&class_owner, state, reference),
        ) {
            Ok(class) => class,
            Err(error) => return self.runtime_error(output, compiled, stack, error.into_message()),
        };
        if let Err(message) = validate_object_mvp(&runtime_class) {
            return self.runtime_error(output, compiled, stack, message);
        }

        let object = ObjectRef::new_with_display_name(&runtime_class, class.display_name.clone());
        let caller_scope = current_scope_class(compiled, stack);
        let constructor = match lookup_resolved_method_in_state(
            compiled,
            state,
            &class.name,
            "__construct",
            caller_scope.as_deref(),
        ) {
            Ok(constructor) => constructor,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        if let Some(constructor) = constructor {
            if let Err(message) = validate_constructor_callable_in_state_scope(
                compiled,
                state,
                caller_scope.as_deref(),
                &constructor.class,
                &constructor.method,
            ) {
                return self.runtime_error(output, compiled, stack, message);
            }
            let class_owner = class_owner_in_state(compiled, state, &constructor.class.name);
            let result = self.execute_function(
                &class_owner,
                constructor.method.function,
                FunctionCall::new(args, Vec::new())
                    .with_call_site_strict_types(compiled.unit().strict_types)
                    .with_call_span(call_span)
                    .with_this(object.clone())
                    .with_class_context_handles(
                        self.class_name_handles(&constructor.class.name).normalized,
                        object_called_class_handle(&object),
                        self.class_name_handles(&constructor.class.name).normalized,
                    ),
                output,
                stack,
                state,
            );
            if !result.status.is_success() || result.fiber_suspension.is_some() {
                return result;
            }
            self.register_destructor_if_needed(compiled, &class, object.clone(), state);
            return VmResult::success_with_diagnostics_no_output(
                Some(Value::Object(object)),
                result.diagnostics,
            );
        }
        if !args.is_empty() {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_TOO_MANY_ARGS: constructor for class {class_name} does not accept arguments"
                ),
            );
        }
        self.register_destructor_if_needed(compiled, &class, object.clone(), state);
        VmResult::success_no_output(Some(Value::Object(object)))
    }

    pub(super) fn preflight_reflection_class_method(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        method: &str,
        values: &[Value],
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        if normalize_class_name(&object.class_name()) != "reflectionclass" {
            return Ok(());
        }
        match normalize_method_name(method).as_str() {
            "getproperty" => {
                let Some(property) = values.first().and_then(reflection_value_string) else {
                    return Ok(());
                };
                let Some((class_name, _property)) = property.split_once("::") else {
                    return Ok(());
                };
                let class_name = normalize_class_name(class_name);
                let exists = self.class_like_exists_with_autoload_cache(
                    compiled,
                    &class_name,
                    AutoloadClassLookupKind::Class,
                    true,
                    None,
                    output,
                    stack,
                    state,
                )?;
                if exists {
                    Ok(())
                } else {
                    Err(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_REFLECTION_UNKNOWN_CLASS: Class \"{class_name}\" does not exist"
                        ),
                    ))
                }
            }
            "implementsinterface" => {
                let Some(interface_name) = values.first().and_then(reflection_value_string) else {
                    return Ok(());
                };
                let exists = self.class_like_exists_with_autoload_cache(
                    compiled,
                    &interface_name,
                    AutoloadClassLookupKind::Interface,
                    true,
                    None,
                    output,
                    stack,
                    state,
                )?;
                if exists {
                    Ok(())
                } else {
                    Err(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_REFLECTION_UNKNOWN_CLASS: Interface \"{interface_name}\" does not exist"
                        ),
                    ))
                }
            }
            _ => Ok(()),
        }
    }

    pub(super) fn autoload_class(
        &self,
        compiled: &CompiledUnit,
        class_name: &str,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        trace_origin: Option<AutoloadTraceOrigin>,
    ) -> Result<(), VmResult> {
        let normalized = normalize_class_name(class_name);
        if lookup_class_in_state(compiled, state, class_name).is_some()
            || state.autoload_stack.iter().any(|name| name == &normalized)
            || !is_valid_autoload_class_name(class_name)
        {
            return Ok(());
        }
        let callbacks = state.autoload_registry.callbacks().to_vec();
        if !callbacks.is_empty() {
            self.record_counter_autoload();
        }
        state.autoload_stack.push(normalized.clone());
        for callback in callbacks {
            let callback_for_trace = trace_origin.map(|_| callback.clone());
            let result = self.call_callable(
                compiled,
                Value::Callable(Box::new(callback)),
                vec![CallArgument::positional(Value::string(
                    class_name.as_bytes().to_vec(),
                ))],
                output,
                stack,
                state,
            );
            if !result.status.is_success() {
                if let (Some(origin), Some(callback)) = (trace_origin, callback_for_trace.as_ref())
                {
                    state.pending_trace = Some(capture_autoload_trace(
                        compiled, stack, callback, class_name, origin,
                    ));
                }
                let _ = state.autoload_stack.pop();
                return Err(result);
            }
            if lookup_class_in_state(compiled, state, class_name).is_some() {
                break;
            }
        }
        let _ = state.autoload_stack.pop();
        Ok(())
    }

    /// Runs the registered-autoloader protocol for a declaration that is
    /// missing from the RUNTIME declaration table, regardless of unit static
    /// tables. The inferred-trait gate needs this: the including unit's own
    /// static table always contains the compile-time-composed trait, which
    /// would short-circuit `autoload_class` before any callback runs.
    pub(super) fn autoload_runtime_missing_declaration(
        &self,
        compiled: &CompiledUnit,
        class_name: &str,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        let normalized = normalize_class_name(class_name);
        if dynamic_class_entry_by_normalized_name(state, &normalized).is_some()
            || state.autoload_stack.iter().any(|name| name == &normalized)
            || !is_valid_autoload_class_name(class_name)
        {
            return Ok(());
        }
        let callbacks = state.autoload_registry.callbacks().to_vec();
        if !callbacks.is_empty() {
            self.record_counter_autoload();
        }
        state.autoload_stack.push(normalized.clone());
        for callback in callbacks {
            let result = self.call_callable(
                compiled,
                Value::Callable(Box::new(callback)),
                vec![CallArgument::positional(Value::string(
                    class_name.as_bytes().to_vec(),
                ))],
                output,
                stack,
                state,
            );
            if !result.status.is_success() {
                let _ = state.autoload_stack.pop();
                return Err(result);
            }
            if dynamic_class_entry_by_normalized_name(state, &normalized).is_some() {
                break;
            }
        }
        let _ = state.autoload_stack.pop();
        Ok(())
    }

    pub(super) fn autoload_class_parents_if_missing(
        &self,
        compiled: &CompiledUnit,
        class: &php_ir::module::ClassEntry,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        self.autoload_class_parents_if_missing_inner(
            compiled,
            class,
            output,
            stack,
            state,
            &mut Vec::new(),
        )
    }

    pub(super) fn autoload_class_parents_if_missing_inner(
        &self,
        compiled: &CompiledUnit,
        class: &php_ir::module::ClassEntry,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        seen: &mut Vec<String>,
    ) -> Result<(), VmResult> {
        let normalized = normalize_class_name(&class.name);
        if seen.iter().any(|name| name == &normalized) {
            return Ok(());
        }
        seen.push(normalized);
        if let Some(parent_name) = class.parent.as_deref() {
            if lookup_class_in_state(compiled, state, parent_name).is_none()
                && internal_runtime_class_entry(&normalize_class_name(parent_name)).is_none()
            {
                let autoload_name = class.parent_display_name.as_deref().unwrap_or(parent_name);
                self.autoload_class(compiled, autoload_name, output, stack, state, None)?;
            }
            if let Some(parent) = lookup_class_in_state(compiled, state, parent_name) {
                self.autoload_class_parents_if_missing_inner(
                    compiled, &parent, output, stack, state, seen,
                )?;
            }
        }
        for interface_name in &class.interfaces {
            if lookup_class_in_state(compiled, state, interface_name).is_none()
                && internal_runtime_class_entry(&normalize_class_name(interface_name)).is_none()
            {
                let display = class_dependency_display_name(compiled, class, interface_name);
                self.autoload_class(compiled, &display, output, stack, state, None)?;
            }
            if let Some(interface) = lookup_class_in_state(compiled, state, interface_name) {
                self.autoload_class_parents_if_missing_inner(
                    compiled, &interface, output, stack, state, seen,
                )?;
            }
        }
        seen.pop();
        Ok(())
    }

    pub(super) fn read_class_constant_static_property_target(
        &self,
        compiled: &CompiledUnit,
        target: ClassConstantStaticPropertyCacheTarget,
        stack: &CallStack,
        state: &mut ExecutionState,
    ) -> Result<ClassStaticCacheRead, String> {
        let (owner, kind, declaring_class_name, member) = match target {
            ClassConstantStaticPropertyCacheTarget::CurrentUnit {
                kind,
                resolved_class: _,
                declaring_class,
                member,
            } => (compiled.clone(), kind, declaring_class, member),
            ClassConstantStaticPropertyCacheTarget::DynamicUnit {
                unit_index,
                kind,
                resolved_class: _,
                declaring_class,
                member,
            } => {
                let Some(owner) = state.dynamic_units.get(unit_index).cloned() else {
                    return Ok(ClassStaticCacheRead::Fallback);
                };
                (owner, kind, declaring_class, member)
            }
        };
        let Some(class) = owner.lookup_class(&declaring_class_name) else {
            return Ok(ClassStaticCacheRead::Fallback);
        };
        match kind {
            ClassConstantStaticPropertyCacheKind::ClassConstant => {
                let Some(constant) = class.constants.iter().find(|entry| entry.name == member)
                else {
                    return Ok(ClassStaticCacheRead::Fallback);
                };
                if validate_constant_access(&owner, stack, class, constant).is_err() {
                    return Ok(ClassStaticCacheRead::Fallback);
                }
                let value = match constant.value {
                    Some(value) => constant_value(owner.unit(), value)?,
                    None => {
                        if let Some(reference) = &constant.value_class_constant {
                            class_constant_reference_value(compiled, state, reference)?
                        } else if let Some(reference) = &constant.value_named_constant {
                            named_constant_reference_value(compiled, state, reference)?
                        } else {
                            Value::Null
                        }
                    }
                };
                Ok(ClassStaticCacheRead::Value(value))
            }
            ClassConstantStaticPropertyCacheKind::EnumCase => {
                let Some(case) = class
                    .enum_cases
                    .iter()
                    .find(|case| case.name.eq_ignore_ascii_case(&member))
                else {
                    return Ok(ClassStaticCacheRead::Fallback);
                };
                let object = enum_case_object(&owner, state, class, case, &|value| {
                    constant_value(owner.unit(), value)
                })?;
                Ok(ClassStaticCacheRead::Value(Value::Object(object)))
            }
            ClassConstantStaticPropertyCacheKind::StaticProperty => {
                let Some(property) = class.properties.iter().find(|entry| entry.name == member)
                else {
                    return Ok(ClassStaticCacheRead::Fallback);
                };
                if !property.flags.is_static {
                    return Ok(ClassStaticCacheRead::Fallback);
                }
                if validate_property_access_in_state(&owner, state, stack, class, property).is_err()
                {
                    return Ok(ClassStaticCacheRead::Fallback);
                }
                let key = static_property_key(class, property);
                if !state.static_properties.contains_key(&key) {
                    let default = static_property_default(&owner, state, stack, class, property)?;
                    state.static_properties.insert(key.clone(), default);
                }
                let value = state
                    .static_properties
                    .get(&key)
                    .cloned()
                    .unwrap_or(Value::Uninitialized);
                if matches!(value, Value::Uninitialized) {
                    return Ok(ClassStaticCacheRead::Fallback);
                }
                Ok(ClassStaticCacheRead::Value(value))
            }
        }
    }
}

pub(super) fn register_dynamic_unit(
    state: &mut ExecutionState,
    compiled: &CompiledUnit,
    unit: CompiledUnit,
    load_kind: DeclarationLoadKind,
) -> usize {
    // Retain the owner before publishing symbol-table entries so every
    // indexed declaration always points at an already-valid unit slot.
    let unit_index = state.push_dynamic_unit(unit.clone());
    for entry in unit.function_table() {
        if state.dynamic_function_index.contains_key(&entry.name) {
            continue;
        }
        state.push_dynamic_function(DynamicFunctionEntry {
            name: entry.name.clone(),
            unit_index,
            function: entry.function,
            origin: function_declaration_origin(&unit, entry.function, &entry.name, load_kind),
        });
    }
    register_dynamic_classes(state, unit_index, &unit, load_kind);
    for entry in unit.constant_table() {
        if dynamic_constant_declared(compiled, state, &entry.name) {
            continue;
        }
        state.push_dynamic_constant(DynamicConstantEntry {
            name: entry.name.clone(),
            unit_index,
            value: entry.value,
            origin: constant_declaration_origin(&unit, &entry.name, load_kind),
        });
    }
    // Class/autoload epochs guard request-local caches and always advance;
    // the LOOKUP epoch stays constant when this thread has already observed
    // this exact unit's declarations (identical replay), so cross-request
    // slot entries keep validating.
    let replayed = state.worker_symbol_epoch
        && WORKER_SYMBOL_LEDGER
            .with(|ledger| !ledger.seen_units.borrow_mut().insert(unit.cache_identity()));
    state.class_table_epoch = state.class_table_epoch.saturating_add(1);
    if !replayed {
        state.bump_lookup_epoch();
    }
    unit_index
}

pub(super) fn dynamic_constant_declared(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    name: &str,
) -> bool {
    compiled.lookup_constant(name).is_some()
        || state.user_constants.contains_key(name)
        || state.dynamic_constant_index.contains_key(name)
}

pub(super) fn validate_dynamic_declarations(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    unit: &CompiledUnit,
) -> Result<(), String> {
    for entry in unit.function_table() {
        if compiled.lookup_function(&entry.name).is_some()
            || BuiltinRegistry::new().contains(&entry.name)
        {
            return Err(format!(
                "E_PHP_VM_FUNCTION_REDECLARATION: Cannot redeclare function {}()",
                entry.name
            ));
        }
        if let Some(existing) = dynamic_function_entry_by_normalized_name(state, &entry.name) {
            return Err(format!(
                "E_PHP_VM_FUNCTION_REDECLARATION: Cannot redeclare function {}() previously declared at {}",
                entry.name,
                existing.origin.display_site()
            ));
        }
    }
    for class in unit.class_table() {
        if is_lowered_internal_interface_skeleton(class) {
            continue;
        }
        if compiled.lookup_class(&class.name).is_some() {
            return Err(format!(
                "E_PHP_VM_CLASS_REDECLARATION: Cannot declare class {}, because the name is already in use",
                class.display_name
            ));
        }
        if let Some(existing) = dynamic_class_entry_in_state(state, &class.name) {
            return Err(format!(
                "E_PHP_VM_CLASS_REDECLARATION: Cannot declare class {}, because the name is already in use; previous declaration at {}",
                class.display_name,
                existing.origin.display_site()
            ));
        }
    }
    Ok(())
}

pub(super) fn retain_dynamic_closure_unit(state: &mut ExecutionState, unit: CompiledUnit) -> usize {
    state.push_dynamic_unit(unit)
}

pub(super) fn dynamic_or_retain_unit_index(
    state: &mut ExecutionState,
    compiled: &CompiledUnit,
) -> usize {
    dynamic_unit_index_for_compiled(state, compiled)
        .unwrap_or_else(|| retain_dynamic_closure_unit(state, compiled.clone()))
}

pub(super) fn dynamic_unit_index_for_compiled(
    state: &ExecutionState,
    compiled: &CompiledUnit,
) -> Option<usize> {
    let identity = compiled.cache_identity();
    if let Some(&index) = state.dynamic_unit_index.get(&identity)
        && state
            .dynamic_units
            .get(index)
            .is_some_and(|unit| unit.ptr_eq(compiled))
    {
        return Some(index);
    }
    // Cache identities are process-unique. Retain a pointer-only collision
    // fallback so integer wraparound can never select the wrong unit.
    state
        .dynamic_units
        .iter()
        .rposition(|unit| unit.ptr_eq(compiled))
}

pub(super) fn function_declaration_origin(
    compiled: &CompiledUnit,
    function: FunctionId,
    name: &str,
    load_kind: DeclarationLoadKind,
) -> DeclarationOrigin {
    let span = compiled
        .unit()
        .functions
        .get(function.index())
        .map_or(IrSpan::default(), |function| function.span);
    declaration_origin(compiled, span, name, DeclarationKind::Function, load_kind)
}

pub(super) fn constant_declaration_origin(
    compiled: &CompiledUnit,
    name: &str,
    load_kind: DeclarationLoadKind,
) -> DeclarationOrigin {
    let span = compiled
        .unit()
        .constant_table
        .iter()
        .find(|entry| entry.name == name)
        .map_or(IrSpan::default(), |entry| entry.span);
    declaration_origin(
        compiled,
        span,
        name,
        DeclarationKind::GlobalConstant,
        load_kind,
    )
}

pub(super) fn declaration_origin(
    compiled: &CompiledUnit,
    span: IrSpan,
    name: &str,
    kind: DeclarationKind,
    load_kind: DeclarationLoadKind,
) -> DeclarationOrigin {
    let (source_path, line) = source_span_file_line(compiled, span).unwrap_or_else(|| {
        let source_path = compiled
            .unit()
            .files
            .get(span.file.index())
            .map_or_else(|| "<unknown>".to_string(), |file| file.path.clone());
        (source_path, i64::from(span.start))
    });
    DeclarationOrigin {
        source_path,
        line,
        span,
        namespace: declaration_namespace(name),
        kind,
        load_kind,
    }
}

pub(super) fn declaration_namespace(name: &str) -> Option<String> {
    let trimmed = name.trim_start_matches('\\');
    let (namespace, _) = trimmed.rsplit_once('\\')?;
    if namespace.is_empty() {
        None
    } else {
        Some(namespace.to_owned())
    }
}

pub(super) fn declare_runtime_function(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    function_name: &str,
    function: FunctionId,
) -> Result<(), String> {
    let normalized = normalize_function_name(function_name);
    if let Some(existing) = compiled.lookup_function(&normalized) {
        return Err(format!(
            "E_PHP_VM_FUNCTION_REDECLARATION: Cannot redeclare function {normalized}(){}",
            function_previous_declaration_suffix(compiled, existing)
        ));
    }
    if let Some(existing) = dynamic_function_entry_by_normalized_name(state, &normalized) {
        return Err(format!(
            "E_PHP_VM_FUNCTION_REDECLARATION: Cannot redeclare function {normalized}() (previously declared in {}:{})",
            existing.origin.source_path, existing.origin.line
        ));
    }
    if BuiltinRegistry::new().contains(&normalized) {
        return Err(format!(
            "E_PHP_VM_FUNCTION_REDECLARATION: Cannot redeclare function {normalized}()"
        ));
    }
    let unit_index = dynamic_or_retain_unit_index(state, compiled);
    state.push_dynamic_function(DynamicFunctionEntry {
        name: normalized,
        unit_index,
        function,
        origin: function_declaration_origin(
            compiled,
            function,
            function_name,
            DeclarationLoadKind::Conditional,
        ),
    });
    state.bump_lookup_epoch();
    Ok(())
}

pub(super) fn function_previous_declaration_suffix(
    compiled: &CompiledUnit,
    function: FunctionId,
) -> String {
    let Some(function) = compiled.unit().functions.get(function.index()) else {
        return String::new();
    };
    let Some((file, line)) = source_span_file_line(compiled, function.span) else {
        return String::new();
    };
    format!(" (previously declared in {file}:{line})")
}

pub(super) fn register_dynamic_classes(
    state: &mut ExecutionState,
    unit_index: usize,
    compiled: &CompiledUnit,
    load_kind: DeclarationLoadKind,
) {
    // Declarations living in compile-time-inferred linked files stand in for
    // what an autoloader would provide at class-link time; pre-registering
    // them would make them visible without any autoloader having run. Their
    // runtime declaration is decided by the linked-entry autoload gate.
    let inferred_files: std::collections::HashSet<php_ir::ids::FileId> = compiled
        .unit()
        .linked_file_entries
        .iter()
        .zip(compiled.unit().linked_entry_inferred_declarations.iter())
        .filter(|(_, inferred)| inferred.is_some())
        .filter_map(|(entry, _)| {
            compiled
                .unit()
                .functions
                .get(entry.index())
                .map(|function| function.span.file)
        })
        .collect();
    for class in compiled.unit().classes.iter().filter(|class| {
        class.span != php_ir::source_map::IrSpan::default()
            && !class.flags.is_conditional
            && !inferred_files.contains(&class.span.file)
    }) {
        let normalized = normalize_class_name(&class.name);
        if state.dynamic_class_index.contains_key(&normalized) {
            continue;
        }
        state.push_dynamic_class(DynamicClassEntry {
            lookup_name: normalized,
            class: Arc::new(class.clone()),
            unit_index,
            origin: declaration_origin(
                compiled,
                class.span,
                &class.name,
                DeclarationKind::ClassLike,
                load_kind,
            ),
        });
    }
}

pub(super) fn is_lowered_internal_interface_skeleton(class: &php_ir::module::ClassEntry) -> bool {
    class.span == php_ir::source_map::IrSpan::default()
        && class.flags.is_interface
        && class.methods.is_empty()
        && class.properties.is_empty()
        && class.constants.is_empty()
        && matches!(
            normalize_class_name(&class.name).as_str(),
            "traversable"
                | "iterator"
                | "iteratoraggregate"
                | "arrayaccess"
                | "throwable"
                | "unitenum"
                | "backedenum"
                | "stringable"
        )
}

pub(super) fn block_first_span(block: &php_ir::block::BasicBlock) -> Option<IrSpan> {
    block
        .instructions
        .first()
        .map(|instruction| instruction.span)
        .or_else(|| block.terminator.as_ref().map(|terminator| terminator.span))
}

pub(super) fn declare_runtime_class(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    class_name: &str,
) -> Result<(), String> {
    let normalized = normalize_class_name(class_name);
    if lookup_class_in_state(compiled, state, &normalized).is_some() {
        return Err(format!(
            "Cannot declare class {class_name}, because the name is already in use"
        ));
    }
    let Some(class) = compiled.lookup_unit_class(&normalized).cloned() else {
        return Err(format!(
            "class declaration metadata for {class_name} is missing"
        ));
    };
    let unit_index = dynamic_or_retain_unit_index(state, compiled);
    state.push_dynamic_class(DynamicClassEntry {
        lookup_name: normalized,
        origin: declaration_origin(
            compiled,
            class.span,
            &class.name,
            DeclarationKind::ClassLike,
            DeclarationLoadKind::Conditional,
        ),
        class: Arc::new(class),
        unit_index,
    });
    state.bump_class_table_epoch();
    Ok(())
}

pub(super) fn dynamic_function_in_state(
    state: &ExecutionState,
    function_name: &str,
) -> Option<(CompiledUnit, FunctionId)> {
    let (unit_index, function) = dynamic_function_target_in_state(state, function_name)?;
    let owner = state.dynamic_units.get(unit_index)?.clone();
    Some((owner, function))
}

pub(super) fn dynamic_function_entry_in_state<'a>(
    state: &'a ExecutionState,
    function_name: &str,
) -> Option<&'a DynamicFunctionEntry> {
    let normalized = normalize_function_name(function_name);
    dynamic_function_entry_by_normalized_name(state, &normalized)
}

pub(super) fn dynamic_function_entry_by_normalized_name<'a>(
    state: &'a ExecutionState,
    normalized_name: &str,
) -> Option<&'a DynamicFunctionEntry> {
    let index = *state.dynamic_function_index.get(normalized_name)?;
    let entry = state.dynamic_functions.get(index)?;
    debug_assert_eq!(entry.name, normalized_name);
    Some(entry)
}

pub(super) fn dynamic_function_target_in_state(
    state: &ExecutionState,
    function_name: &str,
) -> Option<(usize, FunctionId)> {
    let entry = dynamic_function_entry_in_state(state, function_name)?;
    Some((entry.unit_index, entry.function))
}

pub(super) fn dynamic_class_entry_in_state<'a>(
    state: &'a ExecutionState,
    class_name: &str,
) -> Option<&'a DynamicClassEntry> {
    let normalized = normalize_class_name(class_name);
    dynamic_class_entry_by_normalized_name(state, &normalized)
}

/// Cache identity of a request-local dynamic unit slot.
pub(super) fn dynamic_unit_identity(state: &ExecutionState, unit_index: usize) -> u64 {
    state
        .dynamic_units
        .get(unit_index)
        .map_or(0, CompiledUnit::cache_identity)
}

/// Fetches a dynamic unit by its remembered index, validating the unit
/// identity; a mismatch (replay order shifted across requests) re-maps
/// through the state's identity index.
pub(super) fn resolve_dynamic_unit_by_identity(
    state: &ExecutionState,
    unit_index: usize,
    unit_identity: u64,
) -> Option<CompiledUnit> {
    if let Some(unit) = state.dynamic_units.get(unit_index)
        && unit.cache_identity() == unit_identity
    {
        return Some(unit.clone());
    }
    let index = *state.dynamic_unit_index.get(&unit_identity)?;
    state.dynamic_units.get(index).cloned()
}

pub(super) fn dynamic_class_entry_by_normalized_name<'a>(
    state: &'a ExecutionState,
    normalized_name: &str,
) -> Option<&'a DynamicClassEntry> {
    let index = *state.dynamic_class_index.get(normalized_name)?;
    let entry = state.dynamic_classes.get(index)?;
    debug_assert_eq!(entry.lookup_name, normalized_name);
    Some(entry)
}

pub(super) fn dynamic_class_in_loaded_units(
    state: &ExecutionState,
    class_name: &str,
) -> Option<Arc<php_ir::module::ClassEntry>> {
    let normalized = normalize_class_name(class_name);
    state.dynamic_units.iter().rev().find_map(|unit| {
        unit.lookup_class_arc(&normalized)
            .filter(|class| normalize_class_name(&class.name) == normalized)
    })
}

pub(super) fn closure_owner_for_function(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    function: u32,
    debug: Option<&ClosureDebugInfo>,
    owner_unit: Option<usize>,
) -> CompiledUnit {
    let function_id = FunctionId::new(function);
    if let Some(owner_unit) = owner_unit
        && let Some(unit) = state.dynamic_units.get(owner_unit)
        && unit
            .unit()
            .functions
            .get(function_id.index())
            .is_some_and(|entry| entry.flags.is_closure)
    {
        return unit.clone();
    }
    if compiled_unit_contains_closure(compiled, function_id, debug) {
        return compiled.clone();
    }
    state
        .dynamic_units
        .iter()
        .find(|unit| compiled_unit_contains_closure(unit, function_id, debug))
        .cloned()
        .unwrap_or_else(|| compiled.clone())
}

pub(super) fn closure_function_has_this_local(compiled: &CompiledUnit, function: u32) -> bool {
    compiled
        .unit()
        .functions
        .get(FunctionId::new(function).index())
        .is_some_and(|entry| entry.locals.iter().any(|local| local == "this"))
}

pub(super) fn compiled_unit_contains_closure(
    compiled: &CompiledUnit,
    function: FunctionId,
    debug: Option<&ClosureDebugInfo>,
) -> bool {
    let Some(entry) = compiled.unit().functions.get(function.index()) else {
        return false;
    };
    if !entry.flags.is_closure {
        return false;
    }
    let Some(debug) = debug else {
        return true;
    };
    compiled
        .unit()
        .files
        .get(entry.span.file.index())
        .is_some_and(|file| file.path == debug.file)
}

pub(super) fn lookup_class_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
) -> Option<Arc<php_ir::module::ClassEntry>> {
    lookup_class_in_state_ref(compiled, state, class_name).map(ClassLookup::into_arc)
}

pub(super) fn lookup_class_in_state_ref(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
) -> Option<ClassLookup> {
    let normalized = php_ir::module::normalized_class_name(class_name);
    if let Some(entry) = dynamic_class_entry_by_normalized_name(state, &normalized) {
        return Some(ClassLookup::Shared(Arc::clone(&entry.class)));
    }
    if let Some(class) = compiled.lookup_class_arc(class_name) {
        return Some(ClassLookup::Shared(class));
    }
    if let Some(class) = dynamic_class_in_loaded_units(state, &normalized) {
        return Some(ClassLookup::Shared(class));
    }
    if state
        .failed_class_declarations
        .contains(normalized.as_ref())
    {
        return None;
    }
    internal_runtime_class_entry(&normalized)
        .or_else(|| internal_enum_class_entry(&normalized))
        .map(|class| ClassLookup::Owned(Box::new(class)))
}

pub(super) struct ResolvedConstantValue {
    pub(super) value: Value,
    pub(super) predefined: Option<&'static php_std::ConstantDescriptor>,
}

pub(super) fn global_constant_lookup(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    stack: &CallStack,
    name: &str,
) -> Result<Option<ResolvedConstantValue>, String> {
    if let Some(constant) = compiled.lookup_constant(name) {
        Ok(Some(ResolvedConstantValue {
            value: inline_constant_value(constant),
            predefined: None,
        }))
    } else if let Some(value) = dynamic_constant_value_in_state(state, name)? {
        Ok(Some(ResolvedConstantValue {
            value,
            predefined: None,
        }))
    } else if let Some(value) = state.user_constants.get(name) {
        Ok(Some(ResolvedConstantValue {
            value: value.clone(),
            predefined: None,
        }))
    } else if let Some(value) = class_constant_value_by_name(compiled, state, stack, name)? {
        Ok(Some(ResolvedConstantValue {
            value,
            predefined: None,
        }))
    } else {
        Ok(predefined_constant_lookup_for_state(state, name))
    }
}

pub(super) fn global_constant_value(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    stack: &CallStack,
    name: &str,
) -> Result<Option<Value>, String> {
    Ok(global_constant_lookup(compiled, state, stack, name)?.map(|resolved| resolved.value))
}

pub(super) fn lexical_constant_lookup(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    stack: &CallStack,
    name: &str,
) -> Result<Option<ResolvedConstantValue>, String> {
    if let Some(resolved) = global_constant_lookup(compiled, state, stack, name)? {
        return Ok(Some(resolved));
    }
    let Some((_, global_name)) = name.rsplit_once('\\') else {
        return Ok(None);
    };
    if global_name.is_empty() {
        return Ok(None);
    }
    global_constant_lookup(compiled, state, stack, global_name)
}

pub(super) fn lexical_constant_value(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    stack: &CallStack,
    name: &str,
) -> Result<Option<Value>, String> {
    Ok(lexical_constant_lookup(compiled, state, stack, name)?.map(|resolved| resolved.value))
}

fn dynamic_constant_value_in_state(
    state: &ExecutionState,
    name: &str,
) -> Result<Option<Value>, String> {
    let Some(entry_index) = state.dynamic_constant_index.get(name).copied() else {
        return Ok(None);
    };
    let Some(entry) = state.dynamic_constants.get(entry_index) else {
        return Ok(None);
    };
    debug_assert_eq!(entry.name, name);
    let Some(owner) = state.dynamic_units.get(entry.unit_index) else {
        return Ok(None);
    };
    constant_value(owner.unit(), entry.value).map(Some)
}

pub(super) fn class_constant_value_by_name(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    stack: &CallStack,
    name: &str,
) -> Result<Option<Value>, String> {
    let Some((class_name, constant_name)) = name.split_once("::") else {
        return Ok(None);
    };
    let Some(class) = lookup_class_in_state(compiled, state, class_name) else {
        return Ok(None);
    };
    if constant_name.eq_ignore_ascii_case("class") {
        return Ok(Some(Value::String(PhpString::from_test_str(
            &class.display_name,
        ))));
    }
    if let Some(value) = pdo_class_constant_value(class_name, constant_name) {
        return Ok(Some(value));
    }
    if let Some(value) = zip_class_constant_value(class_name, constant_name) {
        return Ok(Some(value));
    }
    if let Some(value) = redis_class_constant_value(class_name, constant_name) {
        return Ok(Some(value));
    }
    if let Some(value) = memcached_class_constant_value(class_name, constant_name) {
        return Ok(Some(value));
    }
    if let Some(value) = intl_class_constant_value(class_name, constant_name) {
        return Ok(Some(value));
    }
    if let Some(value) = xml_class_constant_value(class_name, constant_name) {
        return Ok(Some(value));
    }
    if let Some(value) = date_time_class_constant_value(class_name, constant_name) {
        return Ok(Some(value));
    }
    if let Some(value) =
        spl_class_constant_value_in_state(compiled, state, class_name, constant_name)
    {
        return Ok(Some(value));
    }
    if class.flags.is_enum
        && let Some(case) = class
            .enum_cases
            .iter()
            .find(|case| case.name.eq_ignore_ascii_case(constant_name))
    {
        let object = enum_case_object(compiled, state, &class, case, &|constant| {
            constant_value(compiled.unit(), constant)
        })
        .map_err(|message| {
            format!(
                "E_PHP_VM_ENUM_CASE_CONSTANT_VALUE: failed to build enum case {}::{}: {message}",
                class.display_name, case.name
            )
        })?;
        return Ok(Some(Value::Object(object)));
    }
    let Some((resolved_class, resolved_constant)) = lookup_class_constant_in_state(
        compiled,
        state,
        &class.name,
        &class.display_name,
        constant_name,
    )?
    else {
        return Ok(None);
    };
    validate_constant_access(compiled, stack, &resolved_class, &resolved_constant)?;
    if let Some(value) = resolved_constant.value {
        let owner = class_owner_in_state(compiled, state, &resolved_class.name);
        runtime_constant_value(compiled, state, stack, owner.unit(), value).map(Some)
    } else if let Some(reference) = &resolved_constant.value_class_constant {
        class_constant_reference_value(compiled, state, reference).map(Some)
    } else if let Some(reference) = &resolved_constant.value_named_constant {
        named_constant_reference_value(compiled, state, reference).map(Some)
    } else {
        Ok(Some(Value::Null))
    }
}

pub(super) fn named_constant_reference_value(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    reference: &NamedConstantReference,
) -> Result<Value, String> {
    for name in &reference.names {
        if let Some(constant) = compiled.lookup_constant(name) {
            return Ok(inline_constant_value(constant));
        }
        if let Some(value) = dynamic_constant_value_in_state(state, name)? {
            return Ok(value);
        }
        if let Some(value) = state.user_constants.get(name) {
            return Ok(value.clone());
        }
        if let Some(value) = predefined_constant_value_for_state(state, name) {
            return Ok(value);
        }
    }
    Err(format!(
        "E_PHP_VM_UNDEFINED_CONSTANT: Undefined constant \"{}\"",
        if reference.display_name.is_empty() {
            reference
                .names
                .first()
                .map(String::as_str)
                .unwrap_or_default()
        } else {
            &reference.display_name
        }
    ))
}

pub(super) fn class_constant_reference_value(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    reference: &ClassConstantReference,
) -> Result<Value, String> {
    class_constant_reference_value_inner(compiled, state, reference, &mut Vec::new())
}

fn class_constant_reference_value_inner(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    reference: &ClassConstantReference,
    visiting: &mut Vec<(String, String)>,
) -> Result<Value, String> {
    let key = (
        normalize_class_name(&reference.class_name),
        reference.constant_name.clone(),
    );
    if visiting.iter().any(|entry| entry == &key) {
        return Err(format!(
            "E_PHP_VM_CLASS_CONSTANT_CYCLE: class constant {}::{} has a recursive initializer",
            reference.class_name, reference.constant_name
        ));
    }
    visiting.push(key);
    let (class, constant) = lookup_class_constant_in_state(
        compiled,
        state,
        &reference.class_name,
        &reference.display_class_name,
        &reference.constant_name,
    )?
    .ok_or_else(|| {
        format!(
            "E_PHP_VM_UNKNOWN_CLASS_CONSTANT: Undefined constant {}::{}",
            class_constant_reference_display_class(reference),
            reference.constant_name
        )
    })?;
    let value = if let Some(value) = constant.value {
        let owner = class_owner_in_state(compiled, state, &class.name);
        constant_value(owner.unit(), value)?
    } else if let Some(next) = &constant.value_class_constant {
        class_constant_reference_value_inner(compiled, state, next, visiting)?
    } else if let Some(reference) = &constant.value_named_constant {
        named_constant_reference_value(compiled, state, reference)?
    } else {
        Value::Null
    };
    visiting.pop();
    Ok(value)
}

pub(super) fn lookup_class_constant_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    reference_display_class_name: &str,
    constant_name: &str,
) -> Result<
    Option<(
        Arc<php_ir::module::ClassEntry>,
        php_ir::module::ClassConstantEntry,
    )>,
    String,
> {
    let Some(class) = lookup_class_in_state(compiled, state, class_name) else {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: Class \"{}\" not found",
            if reference_display_class_name.is_empty() {
                display_class_name(class_name)
            } else {
                reference_display_class_name.to_owned()
            }
        ));
    };
    lookup_class_constant_in_state_inner(compiled, state, class, constant_name, &mut Vec::new())
}

fn lookup_class_constant_in_state_inner(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: Arc<php_ir::module::ClassEntry>,
    constant_name: &str,
    seen: &mut Vec<String>,
) -> Result<
    Option<(
        Arc<php_ir::module::ClassEntry>,
        php_ir::module::ClassConstantEntry,
    )>,
    String,
> {
    let normalized = normalize_class_name(&class.name);
    if seen.iter().any(|name| name == &normalized) {
        return Err(format!(
            "E_PHP_VM_CLASS_INHERITANCE_CYCLE: class {} participates in an inheritance cycle",
            class.name
        ));
    }
    seen.push(normalized);
    if let Some(constant) = class
        .constants
        .iter()
        .find(|entry| entry.name == constant_name)
        .cloned()
    {
        seen.pop();
        return Ok(Some((class, constant)));
    }
    if let Some(parent_name) = class.parent.as_deref() {
        let Some(parent) = lookup_class_in_state(compiled, state, parent_name) else {
            return Err(format!(
                "E_PHP_VM_UNKNOWN_PARENT_CLASS: class {} extends missing class {}",
                class.name, parent_name
            ));
        };
        let resolved =
            lookup_class_constant_in_state_inner(compiled, state, parent, constant_name, seen)?;
        if resolved
            .as_ref()
            .is_some_and(|(_, constant)| constant.flags.is_private)
        {
            seen.pop();
            return Ok(None);
        }
        if resolved.is_some() {
            seen.pop();
            return Ok(resolved);
        }
    }
    for interface_name in &class.interfaces {
        let Some(interface) = lookup_class_in_state(compiled, state, interface_name) else {
            continue;
        };
        if let Some(resolved) =
            lookup_class_constant_in_state_inner(compiled, state, interface, constant_name, seen)?
        {
            seen.pop();
            return Ok(Some(resolved));
        }
    }
    seen.pop();
    Ok(None)
}

fn class_constant_reference_display_class(reference: &ClassConstantReference) -> String {
    if reference.display_class_name.is_empty() {
        display_class_name(&reference.class_name)
    } else {
        reference.display_class_name.clone()
    }
}

fn predefined_constant_lookup(name: &str) -> Option<ResolvedConstantValue> {
    let constant = php_std::ExtensionRegistry::standard_library().enabled_constant(name)?;
    Some(ResolvedConstantValue {
        value: php_std::constants::constant_to_value(constant.value()?),
        predefined: Some(constant),
    })
}

fn predefined_constant_lookup_for_state(
    state: &ExecutionState,
    name: &str,
) -> Option<ResolvedConstantValue> {
    match name {
        "PHP_SAPI" => Some(ResolvedConstantValue {
            value: Value::string(state.request.sapi_name.clone()),
            predefined: None,
        }),
        "PHP_BINARY" => Some(ResolvedConstantValue {
            value: Value::string(state.request.php_binary.clone()),
            predefined: None,
        }),
        _ => predefined_constant_lookup(name),
    }
}

fn predefined_constant_value_for_state(state: &ExecutionState, name: &str) -> Option<Value> {
    predefined_constant_lookup_for_state(state, name).map(|resolved| resolved.value)
}
