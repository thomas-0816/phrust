use super::builtin_adapter::BuiltinTypeError;
use super::prelude::*;

struct FilterArrayRequest<'a> {
    name: &'a str,
    input: &'a PhpArray,
    options: Option<&'a Value>,
    add_empty: bool,
    call_span: Option<php_ir::IrSpan>,
}

const VM_FILTER_DEFAULT: i64 = 516;
const VM_FILTER_CALLBACK: i64 = 1_024;
const VM_FILTER_REQUIRE_ARRAY: i64 = 16_777_216;
const VM_FILTER_REQUIRE_SCALAR: i64 = 33_554_432;
const VM_FILTER_FORCE_ARRAY: i64 = 67_108_864;
const VM_FILTER_NULL_ON_FAILURE: i64 = 134_217_728;

#[derive(Clone, Debug, Default)]
struct VmFilterCallbackOptions {
    flags: i64,
    callback: Option<Value>,
    default_value: Option<Value>,
}

pub(super) fn is_filter_callback_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "filter_var" | "filter_input" | "filter_input_array" | "filter_var_array"
    )
}

fn filter_call_needs_vm_callback(name: &str, values: &[Value]) -> bool {
    match name {
        "filter_var" => {
            values
                .get(1)
                .and_then(|value| vm_filter_int(value).ok())
                .unwrap_or(VM_FILTER_DEFAULT)
                == VM_FILTER_CALLBACK
        }
        "filter_input" => {
            values
                .get(2)
                .and_then(|value| vm_filter_int(value).ok())
                .unwrap_or(VM_FILTER_DEFAULT)
                == VM_FILTER_CALLBACK
        }
        "filter_input_array" | "filter_var_array" => {
            filter_array_options_need_vm_callback(values.get(1))
        }
        _ => false,
    }
}

fn filter_array_options_need_vm_callback(options: Option<&Value>) -> bool {
    match options.map(effective_value) {
        Some(Value::Int(filter)) => filter == VM_FILTER_CALLBACK,
        Some(Value::Array(specs)) => specs
            .iter()
            .any(|(_, spec)| vm_filter_spec_filter(spec) == Some(VM_FILTER_CALLBACK)),
        _ => false,
    }
}

fn vm_filter_callback_options(value: Option<&Value>) -> Result<VmFilterCallbackOptions, String> {
    let Some(value) = value else {
        return Ok(VmFilterCallbackOptions::default());
    };
    match effective_value(value) {
        Value::Array(array) => {
            let mut options = VmFilterCallbackOptions::default();
            let flags_key = ArrayKey::String(PhpString::from_test_str("flags"));
            if let Some(value) = array.get(&flags_key) {
                options.flags = vm_filter_int(value)?;
            }
            let options_key = ArrayKey::String(PhpString::from_test_str("options"));
            if let Some(value) = array.get(&options_key) {
                options.callback = Some(value.clone());
            }
            let default_key = ArrayKey::String(PhpString::from_test_str("default"));
            if let Some(value) = array.get(&default_key) {
                options.default_value = Some(value.clone());
            }
            Ok(options)
        }
        other => Ok(VmFilterCallbackOptions {
            flags: vm_filter_int(&other)?,
            ..VmFilterCallbackOptions::default()
        }),
    }
}

fn vm_filter_spec_filter(value: &Value) -> Option<i64> {
    match effective_value(value) {
        Value::Array(array) => {
            let key = ArrayKey::String(PhpString::from_test_str("filter"));
            match array.get(&key) {
                Some(value) => vm_filter_int(value).ok(),
                None => Some(VM_FILTER_DEFAULT),
            }
        }
        other => vm_filter_int(&other).ok(),
    }
}

fn vm_filter_spec_filter_value(value: &Value) -> Value {
    vm_filter_spec_filter(value)
        .map(Value::Int)
        .unwrap_or(Value::Int(VM_FILTER_DEFAULT))
}

fn vm_filter_int(value: &Value) -> Result<i64, String> {
    to_int(value).map_err(|message| format!("E_PHP_RUNTIME_BUILTIN_TYPE: {message}"))
}

fn vm_filter_failure(options: &VmFilterCallbackOptions) -> Value {
    if let Some(default_value) = options.default_value.clone() {
        default_value
    } else if options.flags & VM_FILTER_NULL_ON_FAILURE != 0 {
        Value::Null
    } else {
        Value::Bool(false)
    }
}

fn vm_filter_input_missing_value(options: &VmFilterCallbackOptions) -> Value {
    if let Some(default_value) = options.default_value.clone() {
        default_value
    } else if options.flags & VM_FILTER_NULL_ON_FAILURE != 0 {
        Value::Bool(false)
    } else {
        Value::Null
    }
}

fn filter_callback_type_error_result(
    output: &OutputBuffer,
    compiled: &CompiledUnit,
    stack: &CallStack,
    state: &mut ExecutionState,
    function: &str,
    call_span: Option<php_ir::IrSpan>,
) -> VmResult {
    BuiltinTypeError {
        output,
        compiled,
        stack,
        state,
        function,
        values: &[],
        call_span,
    }
    .result(format!("{function}(): Option must be a valid callback"))
}

fn filter_callback_callable_name(callback: &Value) -> Option<String> {
    match effective_value(callback) {
        Value::String(name) => Some(name.to_string_lossy()),
        Value::Callable(callable) => match *callable {
            CallableValue::UserFunction { name } | CallableValue::InternalBuiltin { name } => {
                Some(name)
            }
            _ => None,
        },
        _ => None,
    }
}

impl Vm {
    pub(super) fn call_filter_callback_builtin(
        &self,
        cursor: ExecutionCursor<'_>,
        name: &str,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
    ) -> VmResult {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let values = match call_builtin_args_to_positional(
            self,
            ExecutionCursor::new(compiled, output, stack, state),
            name,
            args,
            call_span,
        ) {
            Ok(values) => values,
            Err(InternalBuiltinArgError::Message(message)) => {
                return self.runtime_error(output, compiled, stack, message);
            }
            Err(InternalBuiltinArgError::Fatal(result)) => return *result,
        };
        if !filter_call_needs_vm_callback(name, &values) {
            return self.execute_internal_registry_builtin(
                name,
                values,
                call_span,
                ExecutionCursor::new(compiled, output, stack, state),
            );
        }
        let mut diagnostics = Vec::new();
        let result = match name {
            "filter_var" => self.filter_var_callback_value(
                ExecutionCursor::new(compiled, output, stack, state),
                name,
                &values,
                call_span,
                &mut diagnostics,
            ),
            "filter_input" => self.filter_input_callback_value(
                ExecutionCursor::new(compiled, output, stack, state),
                name,
                &values,
                call_span,
                &mut diagnostics,
            ),
            "filter_input_array" => self.filter_input_array_callback_value(
                ExecutionCursor::new(compiled, output, stack, state),
                name,
                &values,
                call_span,
                &mut diagnostics,
            ),
            "filter_var_array" => self.filter_var_array_callback_value(
                ExecutionCursor::new(compiled, output, stack, state),
                name,
                &values,
                call_span,
                &mut diagnostics,
            ),
            _ => Err(Box::new(self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_FILTER_CALLBACK_BUILTIN: {name}"),
            ))),
        };
        match result {
            Ok(value) => VmResult::success_with_diagnostics_no_output(Some(value), diagnostics),
            Err(result) => *result,
        }
    }

    fn filter_var_callback_value(
        &self,
        cursor: ExecutionCursor<'_>,
        name: &str,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        if values.is_empty() || values.len() > 3 {
            return Ok(self
                .execute_internal_registry_builtin(
                    name,
                    values.to_vec(),
                    call_span,
                    ExecutionCursor::new(compiled, output, stack, state),
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
                    ExecutionCursor::new(compiled, output, stack, state),
                )
                .return_value
                .unwrap_or(Value::Null));
        }
        let options = vm_filter_callback_options(values.get(2))
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        self.apply_filter_callback_value(
            ExecutionCursor::new(compiled, output, stack, state),
            name,
            &values[0],
            &options,
            call_span,
            diagnostics,
        )
    }

    fn filter_input_callback_value(
        &self,
        cursor: ExecutionCursor<'_>,
        name: &str,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        if !(2..=5).contains(&values.len()) {
            return Ok(self
                .execute_internal_registry_builtin(
                    name,
                    values.to_vec(),
                    call_span,
                    ExecutionCursor::new(compiled, output, stack, state),
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
                    ExecutionCursor::new(compiled, output, stack, state),
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
            ExecutionCursor::new(compiled, output, stack, state),
            name,
            value,
            &options,
            call_span,
            diagnostics,
        )
    }

    fn filter_input_array_callback_value(
        &self,
        cursor: ExecutionCursor<'_>,
        name: &str,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        if values.is_empty() || values.len() > 3 {
            return Ok(self
                .execute_internal_registry_builtin(
                    name,
                    values.to_vec(),
                    call_span,
                    ExecutionCursor::new(compiled, output, stack, state),
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
            ExecutionCursor::new(compiled, output, stack, state),
            FilterArrayRequest {
                name,
                input: &array,
                options: values.get(1),
                add_empty,
                call_span,
            },
            diagnostics,
        )
    }

    fn filter_var_array_callback_value(
        &self,
        cursor: ExecutionCursor<'_>,
        name: &str,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        if values.is_empty() || values.len() > 3 {
            return Ok(self
                .execute_internal_registry_builtin(
                    name,
                    values.to_vec(),
                    call_span,
                    ExecutionCursor::new(compiled, output, stack, state),
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
                    ExecutionCursor::new(compiled, output, stack, state),
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
            ExecutionCursor::new(compiled, output, stack, state),
            FilterArrayRequest {
                name,
                input: &array,
                options: values.get(1),
                add_empty,
                call_span,
            },
            diagnostics,
        )
    }

    fn apply_filter_callback_array(
        &self,
        cursor: ExecutionCursor<'_>,
        request: FilterArrayRequest<'_>,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let FilterArrayRequest {
            name,
            input,
            options,
            add_empty,
            call_span,
        } = request;
        match options.map(effective_value) {
            None | Some(Value::Null) => Ok(self
                .execute_internal_registry_builtin(
                    name,
                    vec![Value::Array(input.clone())],
                    call_span,
                    ExecutionCursor::new(compiled, output, stack, state),
                )
                .return_value
                .ok_or_else(|| {
                    self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!("E_PHP_VM_FILTER_CALLBACK_RESULT: {name} returned no value"),
                    )
                })?),
            Some(Value::Int(filter)) if filter == VM_FILTER_CALLBACK => {
                let options = VmFilterCallbackOptions::default();
                let mut result = PhpArray::new();
                for (key, value) in input.iter() {
                    let filtered = self.apply_filter_callback_value(
                        ExecutionCursor::new(compiled, output, stack, state),
                        name,
                        value,
                        &options,
                        call_span,
                        diagnostics,
                    )?;
                    result.insert(key.clone(), filtered);
                }
                Ok(Value::Array(result))
            }
            Some(Value::Int(_)) => Ok(self
                .execute_internal_registry_builtin(
                    name,
                    vec![
                        Value::Array(input.clone()),
                        options.cloned().unwrap_or(Value::Null),
                    ],
                    call_span,
                    ExecutionCursor::new(compiled, output, stack, state),
                )
                .return_value
                .ok_or_else(|| {
                    self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!("E_PHP_VM_FILTER_CALLBACK_RESULT: {name} returned no value"),
                    )
                })?),
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
                                ExecutionCursor::new(compiled, output, stack, state),
                                name,
                                value,
                                &options,
                                call_span,
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
                                    ExecutionCursor::new(compiled, output, stack, state),
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
            Some(_) => Ok(self
                .execute_internal_registry_builtin(
                    name,
                    vec![
                        Value::Array(input.clone()),
                        options.cloned().unwrap_or(Value::Null),
                    ],
                    call_span,
                    ExecutionCursor::new(compiled, output, stack, state),
                )
                .return_value
                .ok_or_else(|| {
                    self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!("E_PHP_VM_FILTER_CALLBACK_RESULT: {name} returned no value"),
                    )
                })?),
        }
    }

    fn apply_filter_callback_value(
        &self,
        cursor: ExecutionCursor<'_>,
        name: &str,
        value: &Value,
        options: &VmFilterCallbackOptions,
        call_span: Option<php_ir::IrSpan>,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        if let Value::Array(array) = effective_value(value) {
            if options.flags & VM_FILTER_REQUIRE_SCALAR != 0 {
                return Ok(vm_filter_failure(options));
            }
            let mut result = PhpArray::new();
            for (key, value) in array.iter() {
                result.insert(
                    key.clone(),
                    self.apply_filter_callback_array_value(
                        ExecutionCursor::new(compiled, output, stack, state),
                        name,
                        value,
                        options,
                        call_span,
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
            ExecutionCursor::new(compiled, output, stack, state),
            name,
            value,
            options,
            call_span,
            diagnostics,
        )?;
        if options.flags & VM_FILTER_FORCE_ARRAY != 0 {
            return Ok(Value::Array(PhpArray::from_packed(vec![filtered])));
        }
        Ok(filtered)
    }

    fn apply_filter_callback_array_value(
        &self,
        cursor: ExecutionCursor<'_>,
        name: &str,
        value: &Value,
        options: &VmFilterCallbackOptions,
        call_span: Option<php_ir::IrSpan>,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let Value::Array(array) = effective_value(value) else {
            return self.invoke_filter_callback(
                ExecutionCursor::new(compiled, output, stack, state),
                name,
                value,
                options,
                call_span,
                diagnostics,
            );
        };
        let mut result = PhpArray::new();
        for (key, value) in array.iter() {
            result.insert(
                key.clone(),
                self.apply_filter_callback_array_value(
                    ExecutionCursor::new(compiled, output, stack, state),
                    name,
                    value,
                    options,
                    call_span,
                    diagnostics,
                )?,
            );
        }
        Ok(Value::Array(result))
    }

    fn invoke_filter_callback(
        &self,
        cursor: ExecutionCursor<'_>,
        name: &str,
        value: &Value,
        options: &VmFilterCallbackOptions,
        call_span: Option<php_ir::IrSpan>,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<Value, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let Some(callback) = options.callback.clone() else {
            return Err(Box::new(filter_callback_type_error_result(
                output, compiled, stack, state, name, call_span,
            )));
        };
        if !value_is_callable(compiled, state, &callback, false) {
            return Err(Box::new(filter_callback_type_error_result(
                output, compiled, stack, state, name, call_span,
            )));
        }
        let result = self.call_callable_inner(
            ExecutionCursor::new(compiled, output, stack, state),
            callback.clone(),
            vec![CallArgument::positional(value.clone())],
            call_span,
            true,
            filter_callback_callable_name(&callback),
        );
        if !result.status.is_success() {
            return Err(Box::new(result));
        }
        diagnostics.extend(result.diagnostics);
        Ok(result.return_value.unwrap_or(Value::Null))
    }
}
