use super::prelude::*;

impl Vm {
    pub(super) fn call_filter_callback_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let values = match call_builtin_args_to_positional(
            self, compiled, name, args, call_span, output, stack, state,
        ) {
            Ok(values) => values,
            Err(InternalBuiltinArgError::Message(message)) => {
                return self.runtime_error(output, compiled, stack, message);
            }
            Err(InternalBuiltinArgError::Fatal(result)) => return *result,
        };
        if !filter_call_needs_vm_callback(name, &values) {
            return self.execute_internal_registry_builtin(
                name, values, call_span, output, stack, state, compiled,
            );
        }
        let mut diagnostics = Vec::new();
        let result = match name {
            "filter_var" => self.filter_var_callback_value(
                compiled,
                name,
                &values,
                call_span,
                output,
                stack,
                state,
                &mut diagnostics,
            ),
            "filter_input" => self.filter_input_callback_value(
                compiled,
                name,
                &values,
                call_span,
                output,
                stack,
                state,
                &mut diagnostics,
            ),
            "filter_input_array" => self.filter_input_array_callback_value(
                compiled,
                name,
                &values,
                call_span,
                output,
                stack,
                state,
                &mut diagnostics,
            ),
            "filter_var_array" => self.filter_var_array_callback_value(
                compiled,
                name,
                &values,
                call_span,
                output,
                stack,
                state,
                &mut diagnostics,
            ),
            _ => Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_FILTER_CALLBACK_BUILTIN: {name}"),
            )),
        };
        match result {
            Ok(value) => VmResult::success_with_diagnostics_no_output(Some(value), diagnostics),
            Err(result) => result,
        }
    }

    pub(super) fn filter_var_callback_value(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, VmResult> {
        if values.is_empty() || values.len() > 3 {
            return Ok(self
                .execute_internal_registry_builtin(
                    name,
                    values.to_vec(),
                    call_span,
                    output,
                    stack,
                    state,
                    compiled,
                )
                .return_value
                .unwrap_or(Value::Null));
        }
        let filter = values
            .get(1)
            .map(vm_filter_int)
            .transpose()
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?
            .unwrap_or(VM_FILTER_DEFAULT);
        if filter != VM_FILTER_CALLBACK {
            return Ok(self
                .execute_internal_registry_builtin(
                    name,
                    values.to_vec(),
                    call_span,
                    output,
                    stack,
                    state,
                    compiled,
                )
                .return_value
                .unwrap_or(Value::Null));
        }
        let options = vm_filter_callback_options(values.get(2))
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        self.apply_filter_callback_value(
            compiled,
            name,
            &values[0],
            &options,
            call_span,
            output,
            stack,
            state,
            diagnostics,
        )
    }

    pub(super) fn filter_input_callback_value(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, VmResult> {
        if !(2..=5).contains(&values.len()) {
            return Ok(self
                .execute_internal_registry_builtin(
                    name,
                    values.to_vec(),
                    call_span,
                    output,
                    stack,
                    state,
                    compiled,
                )
                .return_value
                .unwrap_or(Value::Null));
        }
        let source = vm_filter_int(&values[0])
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        let input_name = to_string(&values[1])
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?
            .to_string_lossy();
        let filter = values
            .get(2)
            .map(vm_filter_int)
            .transpose()
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?
            .unwrap_or(VM_FILTER_DEFAULT);
        if filter != VM_FILTER_CALLBACK {
            return Ok(self
                .execute_internal_registry_builtin(
                    name,
                    values.to_vec(),
                    call_span,
                    output,
                    stack,
                    state,
                    compiled,
                )
                .return_value
                .unwrap_or(Value::Null));
        }
        let options = vm_filter_callback_options(values.get(3))
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        let Some(source_array) = self.options.runtime_context.filter_input_array(source) else {
            return Ok(vm_filter_input_missing_value(&options));
        };
        let Some(value) = source_array.get(&php_string_key(&input_name)) else {
            return Ok(vm_filter_input_missing_value(&options));
        };
        self.apply_filter_callback_value(
            compiled,
            name,
            value,
            &options,
            call_span,
            output,
            stack,
            state,
            diagnostics,
        )
    }

    pub(super) fn filter_input_array_callback_value(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, VmResult> {
        if values.is_empty() || values.len() > 3 {
            return Ok(self
                .execute_internal_registry_builtin(
                    name,
                    values.to_vec(),
                    call_span,
                    output,
                    stack,
                    state,
                    compiled,
                )
                .return_value
                .unwrap_or(Value::Null));
        }
        let source = vm_filter_int(&values[0])
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        let Some(array) = self.options.runtime_context.filter_input_array(source) else {
            return Ok(Value::Null);
        };
        if array.is_empty() {
            return Ok(Value::Null);
        }
        let add_empty = values
            .get(2)
            .map(to_bool)
            .transpose()
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?
            .unwrap_or(true);
        self.apply_filter_callback_array(
            compiled,
            name,
            &array,
            values.get(1),
            add_empty,
            call_span,
            output,
            stack,
            state,
            diagnostics,
        )
    }

    pub(super) fn filter_var_array_callback_value(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, VmResult> {
        if values.is_empty() || values.len() > 3 {
            return Ok(self
                .execute_internal_registry_builtin(
                    name,
                    values.to_vec(),
                    call_span,
                    output,
                    stack,
                    state,
                    compiled,
                )
                .return_value
                .unwrap_or(Value::Null));
        }
        let Value::Array(array) = effective_value(&values[0]) else {
            return Ok(self
                .execute_internal_registry_builtin(
                    name,
                    values.to_vec(),
                    call_span,
                    output,
                    stack,
                    state,
                    compiled,
                )
                .return_value
                .unwrap_or(Value::Null));
        };
        let add_empty = values
            .get(2)
            .map(to_bool)
            .transpose()
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?
            .unwrap_or(true);
        self.apply_filter_callback_array(
            compiled,
            name,
            &array,
            values.get(1),
            add_empty,
            call_span,
            output,
            stack,
            state,
            diagnostics,
        )
    }

    pub(super) fn apply_filter_callback_array(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        input: &PhpArray,
        options: Option<&Value>,
        add_empty: bool,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, VmResult> {
        match options.map(effective_value) {
            None | Some(Value::Null) => self
                .execute_internal_registry_builtin(
                    name,
                    vec![Value::Array(input.clone())],
                    call_span,
                    output,
                    stack,
                    state,
                    compiled,
                )
                .return_value
                .ok_or_else(|| {
                    self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!("E_PHP_VM_FILTER_CALLBACK_RESULT: {name} returned no value"),
                    )
                }),
            Some(Value::Int(filter)) if filter == VM_FILTER_CALLBACK => {
                let options = VmFilterCallbackOptions::default();
                let mut result = PhpArray::new();
                for (key, value) in input.iter() {
                    let filtered = self.apply_filter_callback_value(
                        compiled,
                        name,
                        value,
                        &options,
                        call_span,
                        output,
                        stack,
                        state,
                        diagnostics,
                    )?;
                    result.insert(key.clone(), filtered);
                }
                Ok(Value::Array(result))
            }
            Some(Value::Int(_)) => self
                .execute_internal_registry_builtin(
                    name,
                    vec![
                        Value::Array(input.clone()),
                        options.cloned().unwrap_or(Value::Null),
                    ],
                    call_span,
                    output,
                    stack,
                    state,
                    compiled,
                )
                .return_value
                .ok_or_else(|| {
                    self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!("E_PHP_VM_FILTER_CALLBACK_RESULT: {name} returned no value"),
                    )
                }),
            Some(Value::Array(specs)) => {
                let mut result = PhpArray::new();
                for (key, spec) in specs.iter() {
                    match input.get(&key) {
                        Some(value) if vm_filter_spec_filter(spec) == Some(VM_FILTER_CALLBACK) => {
                            let options =
                                vm_filter_callback_options(Some(spec)).map_err(|message| {
                                    self.runtime_error(output, compiled, stack, message)
                                })?;
                            let filtered = self.apply_filter_callback_value(
                                compiled,
                                name,
                                value,
                                &options,
                                call_span,
                                output,
                                stack,
                                state,
                                diagnostics,
                            )?;
                            result.insert(key.clone(), filtered);
                        }
                        Some(value) => {
                            let filtered = self
                                .execute_internal_registry_builtin(
                                    "filter_var",
                                    vec![
                                        value.clone(),
                                        vm_filter_spec_filter_value(spec),
                                        spec.clone(),
                                    ],
                                    call_span,
                                    output,
                                    stack,
                                    state,
                                    compiled,
                                )
                                .return_value
                                .unwrap_or(Value::Null);
                            result.insert(key.clone(), filtered);
                        }
                        None if add_empty => {
                            result.insert(key.clone(), Value::Null);
                        }
                        None => {}
                    }
                }
                Ok(Value::Array(result))
            }
            Some(_) => self
                .execute_internal_registry_builtin(
                    name,
                    vec![
                        Value::Array(input.clone()),
                        options.cloned().unwrap_or(Value::Null),
                    ],
                    call_span,
                    output,
                    stack,
                    state,
                    compiled,
                )
                .return_value
                .ok_or_else(|| {
                    self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!("E_PHP_VM_FILTER_CALLBACK_RESULT: {name} returned no value"),
                    )
                }),
        }
    }

    pub(super) fn apply_filter_callback_value(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        value: &Value,
        options: &VmFilterCallbackOptions,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, VmResult> {
        if let Value::Array(array) = effective_value(value) {
            if options.flags & VM_FILTER_REQUIRE_SCALAR != 0 {
                return Ok(vm_filter_failure(options));
            }
            let mut result = PhpArray::new();
            for (key, value) in array.iter() {
                result.insert(
                    key.clone(),
                    self.apply_filter_callback_array_value(
                        compiled,
                        name,
                        value,
                        options,
                        call_span,
                        output,
                        stack,
                        state,
                        diagnostics,
                    )?,
                );
            }
            return Ok(Value::Array(result));
        }
        if options.flags & VM_FILTER_REQUIRE_ARRAY != 0 {
            return Ok(vm_filter_failure(options));
        }
        let filtered = self.invoke_filter_callback(
            compiled,
            name,
            value,
            options,
            call_span,
            output,
            stack,
            state,
            diagnostics,
        )?;
        if options.flags & VM_FILTER_FORCE_ARRAY != 0 {
            return Ok(Value::Array(PhpArray::from_packed(vec![filtered])));
        }
        Ok(filtered)
    }

    pub(super) fn apply_filter_callback_array_value(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        value: &Value,
        options: &VmFilterCallbackOptions,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, VmResult> {
        let Value::Array(array) = effective_value(value) else {
            return self.invoke_filter_callback(
                compiled,
                name,
                value,
                options,
                call_span,
                output,
                stack,
                state,
                diagnostics,
            );
        };
        let mut result = PhpArray::new();
        for (key, value) in array.iter() {
            result.insert(
                key.clone(),
                self.apply_filter_callback_array_value(
                    compiled,
                    name,
                    value,
                    options,
                    call_span,
                    output,
                    stack,
                    state,
                    diagnostics,
                )?,
            );
        }
        Ok(Value::Array(result))
    }

    pub(super) fn invoke_filter_callback(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        value: &Value,
        options: &VmFilterCallbackOptions,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, VmResult> {
        let Some(callback) = options.callback.clone() else {
            return Err(filter_callback_type_error_result(
                output, compiled, stack, state, name, call_span,
            ));
        };
        if !value_is_callable(compiled, state, &callback, false) {
            return Err(filter_callback_type_error_result(
                output, compiled, stack, state, name, call_span,
            ));
        }
        let result = self.call_callable_inner(
            compiled,
            callback.clone(),
            vec![CallArgument::positional(value.clone())],
            call_span,
            output,
            stack,
            state,
            true,
            filter_callback_callable_name(&callback),
        );
        if !result.status.is_success() {
            return Err(result);
        }
        diagnostics.extend(result.diagnostics);
        Ok(result.return_value.unwrap_or(Value::Null))
    }
}
