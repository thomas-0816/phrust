use super::prelude::*;

impl Vm {
    pub(super) fn clone_object_with_magic(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        class: &php_ir::module::ClassEntry,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<ObjectRef, VmResult> {
        let copy = object.clone_shallow();
        let resolved =
            match lookup_resolved_method_in_state(compiled, state, &class.name, "__clone", None) {
                Ok(Some(method)) => method,
                Ok(None) => return Ok(copy),
                Err(message) => return Err(self.runtime_error(output, compiled, stack, message)),
            };
        if resolved.method.flags.is_static {
            return Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_CLONE_METHOD_INACCESSIBLE: method {}::__clone is not public instance",
                    resolved.class.name
                ),
            ));
        }
        if let Err(message) = validate_method_callable_in_state_scope(
            compiled,
            state,
            current_scope_class(compiled, stack).as_deref(),
            &resolved.class,
            &resolved.method,
        ) {
            return Err(self.runtime_error(output, compiled, stack, message));
        }
        let method_owner = class_owner_in_state(compiled, state, &resolved.class.name);
        let result = self.execute_function(
            &method_owner,
            resolved.method.function,
            FunctionCall::new(Vec::new(), Vec::new())
                .with_this(copy.clone())
                .with_class_context(
                    resolved.class.name.clone(),
                    copy.class_name(),
                    resolved.class.name.clone(),
                ),
            output,
            stack,
            state,
        );
        if !result.status.is_success() {
            return Err(result);
        }
        Ok(copy)
    }

    pub(super) fn register_destructor_if_needed(
        &self,
        compiled: &CompiledUnit,
        class: &php_ir::module::ClassEntry,
        object: ObjectRef,
        state: &mut ExecutionState,
    ) {
        if let Ok(Some(resolved)) = lookup_method_in_hierarchy(compiled, class, "__destruct", None)
            && !resolved.method.flags.is_static
        {
            let visibility = if resolved.method.flags.is_private {
                DestructorVisibility::Private
            } else if resolved.method.flags.is_protected {
                DestructorVisibility::Protected
            } else {
                DestructorVisibility::Public
            };
            state.destructor_queue.register(
                object,
                resolved.class.name.clone(),
                resolved.method.function,
                dynamic_class_owner_index_in_state(state, &resolved.class.name),
                visibility,
            );
        }
    }

    pub(super) fn inaccessible_destructor_message(
        &self,
        compiled: &CompiledUnit,
        state: &ExecutionState,
        entry: &DestructorEntry,
        scope: Option<&str>,
    ) -> Option<String> {
        match entry.visibility {
            DestructorVisibility::Public => None,
            DestructorVisibility::Protected => {
                let allowed = scope
                    .map(|scope| {
                        protected_scope_is_related_in_state(
                            compiled,
                            state,
                            scope,
                            &entry.class_name,
                        )
                        .unwrap_or(false)
                    })
                    .unwrap_or(false);
                (!allowed).then(|| {
                    format!(
                        "E_PHP_VM_PROTECTED_METHOD_ACCESS: Call to protected {}::__destruct() from {}",
                        entry.object.display_name(),
                        scope_description(scope)
                    )
                })
            }
            DestructorVisibility::Private => {
                let declaring_class = normalize_class_name(&entry.class_name);
                let allowed = scope == Some(declaring_class.as_str());
                (!allowed).then(|| {
                    format!(
                        "E_PHP_VM_PRIVATE_METHOD_ACCESS: Call to private {}::__destruct() from {}",
                        entry.object.display_name(),
                        scope_description(scope)
                    )
                })
            }
        }
    }

    pub(super) fn inaccessible_destructor_warning(
        &self,
        compiled: &CompiledUnit,
        stack: &CallStack,
        entry: &DestructorEntry,
    ) -> Option<RuntimeDiagnostic> {
        let visibility = match entry.visibility {
            DestructorVisibility::Public => return None,
            DestructorVisibility::Protected => "protected",
            DestructorVisibility::Private => "private",
        };
        Some(RuntimeDiagnostic::new(
            "E_PHP_VM_DESTRUCTOR_VISIBILITY_WARNING",
            RuntimeSeverity::Warning,
            format!(
                "Call to {visibility} {}::__destruct() from global scope during shutdown ignored",
                entry.object.display_name()
            ),
            RuntimeSourceSpan {
                file: Some("Unknown".to_owned()),
                start: 0,
                end: 0,
            },
            stack_trace(compiled, stack),
            Some(php_runtime::api::PhpReferenceClassification::Warning),
        ))
    }
}
