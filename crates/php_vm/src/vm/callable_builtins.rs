use super::prelude::*;

impl Vm {
    pub(super) fn call_is_callable_builtin(
        &self,
        compiled: &CompiledUnit,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let mut bound: Vec<Option<CallArgument>> = vec![None, None, None];
        let mut positional_index = 0usize;
        let mut saw_named = false;
        for arg in args {
            let index = if let Some(name) = arg.name.clone() {
                saw_named = true;
                match name.as_str() {
                    "value" => 0,
                    "syntax_only" => 1,
                    "callable_name" => 2,
                    _ => {
                        return self.runtime_error(
                            output,
                            compiled,
                            stack,
                            format!(
                                "E_PHP_VM_UNKNOWN_NAMED_ARG: function is_callable has no builtin parameter ${name}"
                            ),
                        );
                    }
                }
            } else {
                if saw_named {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_POSITIONAL_AFTER_NAMED_ARG: function is_callable cannot use positional argument after named argument",
                    );
                }
                let index = positional_index;
                positional_index += 1;
                index
            };
            if index >= bound.len() {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_TOO_MANY_ARGS: is_callable() expects at most 3 arguments",
                );
            }
            if bound[index].is_some() {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_DUPLICATE_NAMED_ARG: is_callable() argument was already provided",
                );
            }
            bound[index] = Some(arg);
        }
        let Some(value_arg) = bound[0].as_ref() else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_ARITY: is_callable expects value",
            );
        };
        let value = value_arg.value.clone();
        let syntax_only = bound[1]
            .as_ref()
            .is_some_and(|arg| to_bool(&arg.value).unwrap_or(false));
        if !syntax_only
            && let Err(result) =
                self.autoload_callable_class(compiled, &value, output, stack, state)
        {
            return *result;
        }
        let callable = value_is_callable(compiled, state, &value, syntax_only);
        if let Some(name_arg) = bound[2].as_ref() {
            match call_argument_reference_cell(compiled, Some(state), name_arg, stack) {
                Ok(Some(cell)) => {
                    let name = if callable {
                        callable_name_for_is_callable(compiled, state, &value)
                            .unwrap_or_else(|| String::from("Array"))
                    } else {
                        String::new()
                    };
                    cell.set(Value::string(name));
                }
                Ok(None) => {}
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            }
        }
        VmResult::success_no_output(Some(Value::Bool(callable)))
    }

    pub(super) fn call_compact_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &CallStack,
    ) -> VmResult {
        let Some(frame) = stack.current() else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_COMPACT_FRAME: compact requires an active frame",
            );
        };
        let Some(function) = compiled.unit().functions.get(frame.function.index()) else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_COMPACT_FRAME: invalid function {}",
                    frame.function.raw()
                ),
            );
        };

        let mut names = Vec::new();
        for value in values {
            collect_compact_variable_names(&value, &mut names);
        }

        let mut result = PhpArray::new();
        for name in names {
            let Some((index, _)) = function
                .locals
                .iter()
                .enumerate()
                .find(|(_, local)| local.as_str() == name.as_str())
            else {
                continue;
            };
            let Some(slot) = frame.locals.get_slot(LocalId::new(index as u32)) else {
                continue;
            };
            if slot.is_uninitialized() {
                continue;
            }
            result.insert(
                ArrayKey::String(PhpString::from_test_str(&name)),
                effective_value(&slot.read()),
            );
        }

        VmResult::success_no_output(Some(Value::Array(result)))
    }

    pub(super) fn call_get_defined_vars_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &CallStack,
    ) -> VmResult {
        if !values.is_empty() {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_SYMBOL_ARITY: get_defined_vars expects no arguments",
            );
        }
        let Some(frame) = stack.current() else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_DEFINED_VARS_FRAME: get_defined_vars requires an active frame",
            );
        };
        let Some(function) = compiled.unit().functions.get(frame.function.index()) else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_DEFINED_VARS_FRAME: invalid function {}",
                    frame.function.raw()
                ),
            );
        };

        let mut result = PhpArray::new();
        for (index, name) in function.locals.iter().enumerate() {
            let Some(slot) = frame.locals.get_slot(LocalId::new(index as u32)) else {
                continue;
            };
            if slot.is_uninitialized() {
                continue;
            }
            result.insert(
                ArrayKey::String(PhpString::from_test_str(name)),
                effective_value(&slot.read()),
            );
        }

        VmResult::success_no_output(Some(Value::Array(result)))
    }

    pub(super) fn call_user_func_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let Some(callback) = values.first().cloned() else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_CALLABLE_ARITY: call_user_func expects at least one argument",
            );
        };
        let args = values
            .into_iter()
            .skip(1)
            .map(CallArgument::positional)
            .collect();
        if let Err(result) = self.preflight_user_callback(
            compiled,
            &callback,
            "call_user_func",
            output,
            stack,
            state,
        ) {
            return *result;
        }
        self.call_callable_inner(
            ExecutionCursor::new(compiled, output, stack, state),
            callback,
            args,
            call_span,
            true,
            None,
        )
    }

    pub(super) fn call_user_func_array_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if values.len() != 2 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_CALLABLE_ARITY: call_user_func_array expects two arguments",
            );
        }
        let Ok([callback, arguments]) = <Vec<Value> as TryInto<[Value; 2]>>::try_into(values)
        else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_CALLABLE_ARITY: call_user_func_array expects two arguments",
            );
        };
        let Value::Array(array) = callable_resolve_reference(arguments) else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_CALLABLE_TYPE: call_user_func_array expects array arguments",
            );
        };
        self.record_counter_cufa_argument_path(!array.is_shared());
        let args = call_args_from_owned_php_array(array);
        if let Err(result) = self.preflight_user_callback(
            compiled,
            &callback,
            "call_user_func_array",
            output,
            stack,
            state,
        ) {
            return *result;
        }
        self.call_callable_inner(
            ExecutionCursor::new(compiled, output, stack, state),
            callback,
            args,
            call_span,
            true,
            None,
        )
    }

    pub(super) fn preflight_user_callback(
        &self,
        compiled: &CompiledUnit,
        callback: &Value,
        builtin: &str,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), Box<VmResult>> {
        match callback {
            Value::Reference(cell) => {
                let resolved = cell.get();
                self.preflight_user_callback(compiled, &resolved, builtin, output, stack, state)?;
            }
            Value::String(name) => {
                let name = name.to_string_lossy();
                if let Some((class_name, method)) = name.split_once("::") {
                    self.preflight_static_user_callback(
                        ExecutionCursor::new(compiled, output, stack, state),
                        class_name,
                        method,
                        builtin,
                    )?;
                }
            }
            Value::Array(array) => {
                let (Some(target), Some(method)) =
                    (array.get(&ArrayKey::Int(0)), array.get(&ArrayKey::Int(1)))
                else {
                    return Ok(());
                };
                let Some(method) = callable_string_ref(method) else {
                    return Ok(());
                };
                match callable_resolve_reference(target.clone()) {
                    Value::Object(object) => self.preflight_object_user_callback(
                        ExecutionCursor::new(compiled, output, stack, state),
                        &object,
                        &method,
                        builtin,
                    )?,
                    Value::String(class_name) => self.preflight_static_user_callback(
                        ExecutionCursor::new(compiled, output, stack, state),
                        &class_name.to_string_lossy(),
                        &method,
                        builtin,
                    )?,
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn preflight_static_user_callback(
        &self,
        cursor: ExecutionCursor<'_>,
        class_name: &str,
        method: &str,
        builtin: &str,
    ) -> Result<(), Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let exists = self.class_like_exists_with_autoload_cache(
            ExecutionCursor::new(compiled, output, stack, state),
            class_name,
            AutoloadClassLookupKind::Class,
            true,
            None,
        )?;
        if !exists {
            return Err(Box::new(self.callable_type_error(
                ExecutionCursor::new(compiled, output, stack, state),
                format!(
                    "{builtin}(): Argument #1 ($callback) must be a valid callback, class \"{class_name}\" not found"
                ),
                Some(class_name.to_owned()),
                Some("class"),
            )));
        }
        let Some(resolved) =
            lookup_resolved_method_in_state(compiled, state, class_name, method, None)
                .map_err(|message| self.runtime_error(output, compiled, stack, message))?
        else {
            return Ok(());
        };
        self.preflight_resolved_user_callback(
            ExecutionCursor::new(compiled, output, stack, state),
            &resolved.class,
            &resolved.method,
            builtin,
        )
    }

    pub(super) fn preflight_object_user_callback(
        &self,
        cursor: ExecutionCursor<'_>,
        object: &ObjectRef,
        method: &str,
        builtin: &str,
    ) -> Result<(), Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let Some(resolved) =
            lookup_resolved_method_in_state(compiled, state, &object.class_name(), method, None)
                .map_err(|message| self.runtime_error(output, compiled, stack, message))?
        else {
            return Ok(());
        };
        self.preflight_resolved_user_callback(
            ExecutionCursor::new(compiled, output, stack, state),
            &resolved.class,
            &resolved.method,
            builtin,
        )
    }

    pub(super) fn preflight_resolved_user_callback(
        &self,
        cursor: ExecutionCursor<'_>,
        class: &php_ir::module::ClassEntry,
        method: &php_ir::module::ClassMethodEntry,
        builtin: &str,
    ) -> Result<(), Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let visibility = if method.flags.is_private {
            Some("private")
        } else if method.flags.is_protected {
            Some("protected")
        } else {
            None
        };
        let Some(visibility) = visibility else {
            return Ok(());
        };
        let scope = current_scope_class(compiled, stack);
        let accessible = validate_method_callable_in_state_scope(
            compiled,
            state,
            scope.as_deref(),
            class,
            method,
        )
        .is_ok();
        if accessible {
            return Ok(());
        }
        Err(Box::new(self.callable_type_error(
            ExecutionCursor::new(compiled, output, stack, state),
            format!(
                "{builtin}(): Argument #1 ($callback) must be a valid callback, cannot access {visibility} method {}::{}()",
                class.display_name, method.name
            ),
            Some(format!("{}::{}", class.display_name, method.name)),
            Some("function"),
        )))
    }

    pub(super) fn callable_type_error(
        &self,
        cursor: ExecutionCursor<'_>,
        message: String,
        requested_name: Option<String>,
        lookup_kind: Option<&'static str>,
    ) -> VmResult {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        self.runtime_error_with_bringup_context(
            ExecutionView::new(compiled, output, stack, state),
            RuntimeSourceSpan::default(),
            format!("E_PHP_RUNTIME_BUILTIN_TYPE: {message}"),
            BringupDiagnosticInput {
                error_class: Some("callable_resolution"),
                requested_name,
                lookup_kind: lookup_kind.or(Some("function")),
                autoload_enabled: Some(true),
                ..BringupDiagnosticInput::default()
            },
        )
    }

    pub(super) fn call_forward_static_call_builtin(
        &self,
        compiled: &CompiledUnit,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let Some(callback) = values.first().cloned() else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_CALLABLE_ARITY: forward_static_call expects at least one argument",
            );
        };
        if current_scope_class(compiled, stack).is_none() {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_INVALID_STATIC_SCOPE: forward_static_call is not available outside class scope",
            );
        }
        let args = values
            .into_iter()
            .skip(1)
            .map(CallArgument::positional)
            .collect();
        self.call_callable(compiled, callback, args, output, stack, state)
    }

    pub(super) fn call_function_context_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
    ) -> VmResult {
        if matches!(name, "debug_backtrace" | "debug_print_backtrace") {
            return self.call_debug_backtrace_builtin(compiled, name, values, output, stack);
        }
        let Some(frame) = stack.current() else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_CALL_CONTEXT: {name} is not available outside function scope"),
            );
        };
        let Some(function) = compiled.unit().functions.get(frame.function.index()) else {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_CALL_CONTEXT: missing frame function",
            );
        };
        if function.flags.is_top_level {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_CALL_CONTEXT: {name} is not available in top-level scope"),
            );
        }
        match name {
            "func_get_args" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_CALL_CONTEXT_ARITY: func_get_args expects no arguments",
                    );
                }
                VmResult::success_no_output(Some(Value::Array(PhpArray::from_packed(
                    frame.arguments.clone(),
                ))))
            }
            "func_num_args" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_CALL_CONTEXT_ARITY: func_num_args expects no arguments",
                    );
                }
                VmResult::success_no_output(Some(Value::Int(frame.arguments.len() as i64)))
            }
            "func_get_arg" => {
                if values.len() != 1 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_CALL_CONTEXT_ARITY: func_get_arg expects one argument",
                    );
                }
                let index = match to_int(&values[0]) {
                    Ok(index) if index >= 0 => index as usize,
                    Ok(_) => {
                        return self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_CALL_CONTEXT_INDEX: func_get_arg index must be non-negative",
                        );
                    }
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                let Some(value) = frame.arguments.get(index).cloned() else {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!("E_PHP_VM_CALL_CONTEXT_INDEX: func_get_arg index {index} is out of range"),
                    );
                };
                VmResult::success_no_output(Some(value))
            }
            _ => self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_CALL_CONTEXT_BUILTIN: {name}"),
            ),
        }
    }

    pub(super) fn call_debug_backtrace_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &CallStack,
    ) -> VmResult {
        if values.len() > 2 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_CALL_CONTEXT_ARITY: {name} expects zero to two arguments"),
            );
        }
        let options = match values.first() {
            Some(value) => match to_int(value) {
                Ok(value) => value,
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            },
            None => {
                if name == "debug_backtrace" {
                    1
                } else {
                    0
                }
            }
        };
        let limit = match values.get(1) {
            Some(value) => match to_int(value) {
                Ok(value) if value >= 0 => value as usize,
                Ok(_) => {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!("E_PHP_VM_CALL_CONTEXT_LIMIT: {name} limit must be non-negative"),
                    );
                }
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            },
            None => 0,
        };
        if name == "debug_print_backtrace" {
            let text = capture_debug_print_backtrace_string(compiled, stack, options, limit);
            if !text.is_empty() {
                output.write_test_str(&text);
                output.write_bytes(b"\n");
            }
            return VmResult::success_no_output(Some(Value::Null));
        }
        VmResult::success_no_output(Some(Value::Array(debug_backtrace_array(
            compiled, stack, options, limit,
        ))))
    }
}
