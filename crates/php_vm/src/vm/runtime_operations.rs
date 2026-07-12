use super::prelude::*;

pub(super) fn object_has_public_to_string(compiled: &CompiledUnit, object: &ObjectRef) -> bool {
    if is_php_token_runtime_class(&object.class_name()) {
        return true;
    }
    let Some(class) = compiled.lookup_class(&object.class_name()) else {
        return false;
    };
    let Ok(Some(resolved)) = lookup_method_in_hierarchy(compiled, class, "__toString", None) else {
        return false;
    };
    !resolved.method.flags.is_static
        && !resolved.method.flags.is_private
        && !resolved.method.flags.is_protected
}

pub(super) fn object_has_public_to_string_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    object: &ObjectRef,
) -> bool {
    if is_php_token_runtime_class(&object.class_name()) {
        return true;
    }
    let Ok(Some(resolved)) =
        lookup_resolved_method_in_state(compiled, state, &object.class_name(), "__toString", None)
    else {
        return false;
    };
    !resolved.method.flags.is_static
        && !resolved.method.flags.is_private
        && !resolved.method.flags.is_protected
}

pub(super) fn packed_array_get(array: &Value, index: &Value) -> Result<Value, String> {
    let Value::Array(array) = array else {
        return Err("E_PHP_VM_ARRAY_FETCH_TYPE: value is not an array".to_owned());
    };
    let NumericValue::Int(index) = to_number(index)? else {
        return Err("E_PHP_VM_ARRAY_FETCH_INDEX: array index must be int".to_owned());
    };
    if index < 0 {
        return Ok(Value::Null);
    }
    Ok(array
        .packed_element_fast(index as usize)
        .cloned()
        .unwrap_or(Value::Null))
}

fn has_array_operand(value: &Value) -> bool {
    match value {
        Value::Array(_) => true,
        Value::Reference(cell) => has_array_operand(&cell.get()),
        _ => false,
    }
}

fn try_array_union(lhs: &Value, rhs: &Value) -> Option<Value> {
    let Value::Array(lhs) = effective_value(lhs) else {
        return None;
    };
    let Value::Array(rhs) = effective_value(rhs) else {
        return None;
    };
    let mut result = lhs.clone();
    for (key, value) in rhs.iter() {
        if result.get(&key).is_none() {
            result.insert(key.clone(), effective_value(value));
        }
    }
    Some(Value::Array(result))
}

fn unsupported_operand_types_error(
    output: &OutputBuffer,
    compiled: &CompiledUnit,
    stack: &CallStack,
    lhs: &Value,
    op: BinaryOp,
    rhs: &Value,
    source_span: RuntimeSourceSpan,
) -> VmResult {
    let message = format!(
        "Unsupported operand types: {} {} {}",
        value_type_name(lhs),
        binary_operator_symbol(op),
        value_type_name(rhs)
    );
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_UNSUPPORTED_OPERAND_TYPES",
        RuntimeSeverity::FatalError,
        message.clone(),
        source_span,
        stack_trace(compiled, stack),
        Some(php_runtime::api::PhpReferenceClassification::TypeError),
    );
    VmResult::runtime_error_with_diagnostic(output.clone(), message, diagnostic)
}

fn binary_operator_symbol(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::Concat => ".",
        BinaryOp::Pow => "**",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::ShiftLeft => "<<",
        BinaryOp::ShiftRight => ">>",
    }
}

fn emit_power_zero_negative_exponent_deprecation(
    compiled: &CompiledUnit,
    output: &mut OutputBuffer,
    stack: &CallStack,
    state: &mut ExecutionState,
    lhs: NumericValue,
    rhs: NumericValue,
    source_span: RuntimeSourceSpan,
) {
    if lhs.as_f64() != 0.0
        || rhs.as_f64() >= 0.0
        || !error_reporting_allows(state, php_runtime::api::PHP_E_DEPRECATED)
    {
        return;
    }
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_POW_ZERO_NEGATIVE_EXPONENT",
        RuntimeSeverity::Deprecation,
        "Power of base 0 and negative exponent is deprecated",
        source_span,
        stack_trace(compiled, stack),
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

pub(super) fn write_object_numeric_cast_warning(
    output: &mut OutputBuffer,
    state: &mut ExecutionState,
    object: &ObjectRef,
    target: &str,
    source_span: RuntimeSourceSpan,
) {
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_OBJECT_NUMERIC_CAST_WARNING",
        RuntimeSeverity::Warning,
        format!(
            "Object of class {} could not be converted to {target}",
            object.display_name()
        ),
        source_span,
        Vec::new(),
        Some(php_runtime::api::PhpReferenceClassification::Warning),
    );
    emit_vm_diagnostic(
        output,
        state,
        &diagnostic,
        php_runtime::api::PhpDiagnosticChannel::Warning,
        php_runtime::api::PHP_E_WARNING,
    );
    state.diagnostics.push(diagnostic);
}

fn write_non_numeric_value_warning(
    output: &mut OutputBuffer,
    state: &mut ExecutionState,
    source_span: RuntimeSourceSpan,
) {
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_NON_NUMERIC_STRING_WARNING",
        RuntimeSeverity::Warning,
        "A non-numeric value encountered",
        source_span,
        Vec::new(),
        Some(php_runtime::api::PhpReferenceClassification::Warning),
    );
    emit_vm_diagnostic(
        output,
        state,
        &diagnostic,
        php_runtime::api::PhpDiagnosticChannel::Warning,
        php_runtime::api::PHP_E_WARNING,
    );
    state.diagnostics.push(diagnostic);
}

fn non_numeric_string_type_error(
    output: &mut OutputBuffer,
    compiled: &CompiledUnit,
    stack: &CallStack,
    lhs: &Value,
    op: BinaryOp,
    rhs: &Value,
    source_span: RuntimeSourceSpan,
) -> VmResult {
    let message = format!(
        "Unsupported operand types: {} {} {}",
        value_type_name(lhs),
        binary_operator_symbol(op),
        value_type_name(rhs)
    );
    let file = source_span.file.clone().unwrap_or_default();
    let line = runtime_source_span_display_line(compiled, &source_span).unwrap_or(0);
    let trace = capture_backtrace_string(compiled, stack);
    output.write_test_str(&format!(
        "\nFatal error: Uncaught TypeError: {message} in {file}:{line}\nStack trace:\n{trace}\n  thrown in {file} on line {line}\n"
    ));
    let diagnostic_message = format!("E_PHP_RUNTIME_NON_NUMERIC_STRING: {message}");
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_NON_NUMERIC_STRING",
        RuntimeSeverity::FatalError,
        diagnostic_message.clone(),
        source_span,
        stack_trace_from_captured_trace(&trace).unwrap_or_else(|| stack_trace(compiled, stack)),
        Some(php_runtime::api::PhpReferenceClassification::TypeError),
    );
    VmResult::runtime_error_with_diagnostic(output.clone(), diagnostic_message, diagnostic)
}

impl Vm {
    pub(super) fn value_to_string(
        &self,
        compiled: &CompiledUnit,
        value: &Value,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<PhpString, Box<VmResult>> {
        match value {
            Value::Object(object) => {
                self.object_to_string(compiled, object.clone(), output, stack, state)
            }
            Value::Array(_) => {
                self.emit_array_to_string_warning(
                    compiled,
                    output,
                    stack,
                    state,
                    RuntimeSourceSpan::default(),
                )?;
                Ok(PhpString::from_bytes(b"Array".to_vec()))
            }
            Value::Reference(cell) => {
                self.value_to_string(compiled, &cell.get(), output, stack, state)
            }
            other => Ok(to_string(other)
                .map_err(|message| self.runtime_error(output, compiled, stack, message))?),
        }
    }

    pub(super) fn value_to_string_with_source_span(
        &self,
        compiled: &CompiledUnit,
        value: &Value,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        source_span: RuntimeSourceSpan,
    ) -> Result<PhpString, Box<VmResult>> {
        match value {
            Value::Object(object) => self.object_to_string_with_source_span(
                compiled,
                object.clone(),
                output,
                stack,
                state,
                source_span,
            ),
            Value::Array(_) => {
                self.emit_array_to_string_warning(compiled, output, stack, state, source_span)?;
                Ok(PhpString::from_bytes(b"Array".to_vec()))
            }
            Value::Reference(cell) => self.value_to_string_with_source_span(
                compiled,
                &cell.get(),
                output,
                stack,
                state,
                source_span,
            ),
            other => Ok(to_string(other).map_err(|message| {
                self.runtime_error_with_source_span(output, compiled, stack, source_span, message)
            })?),
        }
    }

    pub(super) fn dynamic_property_name(
        &self,
        unit: &IrUnit,
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        property: Operand,
        output: &mut OutputBuffer,
        state: &mut ExecutionState,
    ) -> Result<String, Box<VmResult>> {
        let value = read_operand(unit, stack, property)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        let property = self.value_to_string(compiled, &value, output, stack, state)?;
        Ok(property.to_string_lossy())
    }

    pub(super) fn emit_array_to_string_warning(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        source_span: RuntimeSourceSpan,
    ) -> Result<(), Box<VmResult>> {
        if state.suppress_array_to_string_warnings > 0 {
            return Ok(());
        }
        let diagnostic = array_to_string_warning(source_span, stack_trace(compiled, stack));
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::api::PHP_E_WARNING,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::api::PHP_E_WARNING) {
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::api::PhpDiagnosticChannel::Warning,
                php_runtime::api::PHP_E_WARNING,
            );
            state.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    pub(super) fn object_to_string(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<PhpString, Box<VmResult>> {
        self.object_to_string_with_source_span(
            compiled,
            object,
            output,
            stack,
            state,
            RuntimeSourceSpan::default(),
        )
    }

    pub(super) fn object_has_string_conversion(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &ExecutionState,
    ) -> Result<bool, Box<VmResult>> {
        if is_reflection_runtime_class(&object.class_name())
            || internal_throwable_instanceof(&object.class_name_handle(), "throwable").is_some()
            || spl_runtime_marker(object).is_some_and(|class| is_spl_caching_iterator_class(&class))
            || is_spl_file_runtime_class(&object.class_name())
            || is_php_token_runtime_class(&object.class_name())
            || normalize_class_name(&object.class_name()) == "simplexmlelement"
        {
            return Ok(true);
        }
        let Some(class) = lookup_class_in_state(compiled, state, &object.class_name()) else {
            return Ok(false);
        };
        match lookup_resolved_method_in_state(compiled, state, &class.name, "__toString", None) {
            Ok(Some(resolved)) => Ok(!resolved.method.flags.is_static
                && !resolved.method.flags.is_private
                && !resolved.method.flags.is_protected),
            Ok(None) => Ok(false),
            Err(message) => Err(Box::new(
                self.runtime_error(output, compiled, stack, message),
            )),
        }
    }

    pub(super) fn object_to_string_with_source_span(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        source_span: RuntimeSourceSpan,
    ) -> Result<PhpString, Box<VmResult>> {
        if is_reflection_runtime_class(&object.class_name()) {
            return Ok(reflection_object_to_string(&object)
                .map(|value| PhpString::from(value.into_bytes()))
                .map_err(|message| {
                    self.runtime_error_with_source_span(
                        output,
                        compiled,
                        stack,
                        source_span.clone(),
                        message,
                    )
                })?);
        }
        if internal_throwable_instanceof(&object.class_name_handle(), "throwable").is_some() {
            return Ok(PhpString::from_bytes(
                throwable_string(&object).into_bytes(),
            ));
        }
        if is_php_token_runtime_class(&object.class_name()) {
            return match object.get_property("text") {
                Some(Value::String(text)) => Ok(text),
                _ => Err(Box::new(self.runtime_error_with_source_span(
                    output,
                    compiled,
                    stack,
                    source_span,
                    "E_PHP_VM_TOSTRING_RETURN_TYPE: PhpToken::__toString(): Return value must be of type string",
                ))),
            };
        }
        if spl_runtime_marker(&object).is_some_and(|class| is_spl_caching_iterator_class(&class)) {
            return self.spl_caching_iterator_to_string(
                compiled,
                &object,
                source_span,
                output,
                stack,
                state,
            );
        }
        if is_spl_file_runtime_class(&object.class_name()) {
            return match call_spl_file_method(
                &object,
                "__toString",
                Vec::new(),
                &self.options.runtime_context,
            ) {
                Ok(Value::String(value)) => Ok(value),
                Ok(other) => Err(Box::new(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_VM_TOSTRING_RETURN_TYPE: {}::__toString(): Return value must be of type string, {} returned",
                        object.class_name(),
                        value_type_name(&other)
                    ),
                ))),
                Err(message) => Err(Box::new(self.runtime_error(output, compiled, stack, message))),
            };
        }
        if normalize_class_name(&object.class_name()) == "simplexmlelement" {
            return match php_runtime::api::xml::simplexml_text(&object) {
                Value::String(value) => Ok(value),
                other => Err(Box::new(self.runtime_error_with_source_span(
                    output,
                    compiled,
                    stack,
                    source_span,
                    format!(
                        "E_PHP_VM_TOSTRING_RETURN_TYPE: SimpleXMLElement::__toString(): Return value must be of type string, {} returned",
                        value_type_name(&other)
                    ),
                ))),
            };
        }
        let Some(class) = lookup_class_in_state(compiled, state, &object.class_name()) else {
            return Err(Box::new(self.runtime_error_with_source_span(
                output,
                compiled,
                stack,
                source_span,
                format!(
                    "E_PHP_VM_UNKNOWN_CLASS: class {} is not defined",
                    object.class_name()
                ),
            )));
        };
        let resolved = match lookup_resolved_method_in_state(
            compiled,
            state,
            &class.name,
            "__toString",
            None,
        ) {
            Ok(Some(method)) => method,
            Ok(None) => {
                return Err(Box::new(self.runtime_error_with_source_span(
                    output,
                    compiled,
                    stack,
                    source_span,
                    format!(
                        "E_PHP_RUNTIME_OBJECT_TO_STRING_GAP: Object of class {} could not be converted to string",
                        class.display_name
                    ),
                )));
            }
            Err(message) => {
                return Err(Box::new(self.runtime_error_with_source_span(
                    output,
                    compiled,
                    stack,
                    source_span,
                    message,
                )));
            }
        };
        if resolved.method.flags.is_static
            || resolved.method.flags.is_private
            || resolved.method.flags.is_protected
        {
            return Err(Box::new(self.runtime_error_with_source_span(
                output,
                compiled,
                stack,
                source_span,
                format!(
                    "E_PHP_VM_TOSTRING_INACCESSIBLE: method {}::__toString is not public instance",
                    resolved.class.name
                ),
            )));
        }
        let result = self.call_object_method_callable(
            ExecutionCursor::new(compiled, output, stack, state),
            object.clone(),
            "__toString",
            Vec::new(),
            None,
        );
        if !result.status.is_success() {
            return Err(Box::new(result));
        }
        match result.return_value.unwrap_or(Value::Null) {
            Value::String(value) => Ok(value),
            other => Err(Box::new(self.runtime_error_with_source_span(
                output,
                compiled,
                stack,
                source_span,
                format!(
                    "E_PHP_VM_TOSTRING_RETURN_TYPE: {}::__toString(): Return value must be of type string, {} returned",
                    class.display_name,
                    value_type_name(&other)
                ),
            ))),
        }
    }

    pub(super) fn try_write_echo_fast(output: &mut OutputBuffer, value: &Value) -> bool {
        match Self::echo_append_semantic_helper(value) {
            SemanticHelperResult::FastHit => {
                match value {
                    Value::String(value) => output.write_fast_php_string(value),
                    Value::Null | Value::Bool(false) => {}
                    Value::Bool(true) => output.write_fast_bytes(b"1"),
                    Value::Int(value) => output.write_fast_bytes(value.to_string().as_bytes()),
                    Value::Reference(_) => unreachable!("reference is a fallback"),
                    _ => unreachable!("non-scalar echo is a fallback"),
                }
                true
            }
            SemanticHelperResult::Fallback(_) => false,
        }
    }

    pub(super) fn write_echo(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        value: &Value,
    ) -> Result<(), Box<VmResult>> {
        let _source = layout_source::enter(layout_source::OUTPUT_STRING_CONVERSION);
        if Self::try_write_echo_fast(output, value) {
            return Ok(());
        }
        match Self::echo_append_semantic_helper(value) {
            SemanticHelperResult::FastHit => unreachable!("fast hit already appended"),
            SemanticHelperResult::Fallback(reason) => {
                output.record_slow_append_reason(reason);
                if let Value::Reference(cell) = value {
                    return self.write_echo(compiled, output, stack, state, &cell.get());
                }
                let string = self.value_to_string(compiled, value, output, stack, state)?;
                output.write_php_string(&string);
                Ok(())
            }
        }
    }

    pub(super) fn echo_append_semantic_helper(value: &Value) -> SemanticHelperResult {
        match value {
            Value::String(_)
            | Value::Null
            | Value::Bool(false)
            | Value::Bool(true)
            | Value::Int(_) => SemanticHelperResult::FastHit,
            Value::Reference(_) => SemanticHelperResult::Fallback("reference_deref"),
            Value::Float(_) => SemanticHelperResult::Fallback("float_conversion"),
            Value::Array(_) => SemanticHelperResult::Fallback("array_conversion_warning"),
            Value::Object(_) | Value::Fiber(_) | Value::Generator(_) => {
                SemanticHelperResult::Fallback("object_to_string")
            }
            Value::Resource(_) => SemanticHelperResult::Fallback("resource_conversion"),
            Value::Callable(_) => SemanticHelperResult::Fallback("callable_conversion_error"),
            Value::Uninitialized => {
                SemanticHelperResult::Fallback("uninitialized_conversion_error")
            }
        }
    }

    pub(super) fn resolve_exit_value(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        value: Option<Value>,
    ) -> Result<i32, Box<VmResult>> {
        match value {
            Some(Value::Int(code)) => Ok(normalize_exit_code(code)),
            Some(value) => {
                self.write_echo(compiled, output, stack, state, &value)?;
                Ok(0)
            }
            None => Ok(0),
        }
    }

    pub(super) fn execute_binary(
        &self,
        cursor: ExecutionCursor<'_>,
        op: BinaryOp,
        lhs: &Value,
        rhs: &Value,
        source_span: RuntimeSourceSpan,
    ) -> Result<Value, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        match op {
            BinaryOp::Concat => {
                if let Some(reason) = concat_fallback_reason(lhs, rhs) {
                    self.record_counter_concat_fallback(reason);
                }
                let lhs = self.value_to_string_with_source_span(
                    compiled,
                    lhs,
                    output,
                    stack,
                    state,
                    source_span.clone(),
                )?;
                let rhs = self.value_to_string_with_source_span(
                    compiled,
                    rhs,
                    output,
                    stack,
                    state,
                    source_span,
                )?;
                if lhs.len().checked_add(rhs.len()).is_some() {
                    self.record_counter_concat_prealloc_hit();
                    Ok(Value::String(PhpString::from_parts(&[
                        lhs.as_bytes(),
                        rhs.as_bytes(),
                    ])))
                } else {
                    self.record_counter_concat_fallback("capacity_overflow");
                    let mut bytes = lhs.into_bytes();
                    bytes.extend_from_slice(rhs.as_bytes());
                    Ok(Value::String(PhpString::from_bytes(bytes)))
                }
            }
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                if op == BinaryOp::Add
                    && let Some(union) = try_array_union(lhs, rhs)
                {
                    return Ok(union);
                }
                if has_array_operand(lhs) || has_array_operand(rhs) {
                    return Err(Box::new(unsupported_operand_types_error(
                        output,
                        compiled,
                        stack,
                        lhs,
                        op,
                        rhs,
                        source_span,
                    )));
                }
                if let Some(value) = self.execute_numeric_string_int_binary(
                    ExecutionCursor::new(compiled, output, stack, state),
                    op,
                    lhs,
                    rhs,
                    source_span.clone(),
                ) {
                    return value;
                }
                let lhs = to_arithmetic_number(lhs)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                let rhs = to_arithmetic_number(rhs)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                if lhs.leading_numeric_string || rhs.leading_numeric_string {
                    write_non_numeric_value_warning(output, state, source_span);
                }
                Ok(execute_arithmetic(op, lhs.value, rhs.value)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?)
            }
            BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::ShiftLeft
            | BinaryOp::ShiftRight => Ok(execute_bitwise(op, lhs, rhs)
                .map_err(|message| self.runtime_error(output, compiled, stack, message))?),
            BinaryOp::Pow => {
                let lhs = to_number(lhs)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                let rhs = to_number(rhs)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                emit_power_zero_negative_exponent_deprecation(
                    compiled,
                    output,
                    stack,
                    state,
                    lhs,
                    rhs,
                    source_span,
                );
                Ok(execute_power(lhs, rhs)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?)
            }
        }
    }

    pub(super) fn execute_numeric_string_int_binary(
        &self,
        cursor: ExecutionCursor<'_>,
        op: BinaryOp,
        lhs: &Value,
        rhs: &Value,
        source_span: RuntimeSourceSpan,
    ) -> Option<Result<Value, Box<VmResult>>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let lhs = effective_value(lhs);
        let rhs = effective_value(rhs);
        let (string, int, string_is_lhs) = match (&lhs, &rhs) {
            (Value::String(string), Value::Int(int)) => (string, *int, true),
            (Value::Int(int), Value::String(string)) => (string, *int, false),
            _ => return None,
        };

        let classified = classify_php_string(string);
        let Some(value) = classified.value else {
            return Some(Err(Box::new(non_numeric_string_type_error(
                output,
                compiled,
                stack,
                &lhs,
                op,
                &rhs,
                source_span,
            ))));
        };
        if classified.kind == NumericStringKind::LeadingNumeric {
            write_non_numeric_value_warning(output, state, source_span);
        }

        let string_number = match value {
            NumericStringValue::Int(value) => NumericValue::Int(value),
            NumericStringValue::Float(value) => NumericValue::Float(value),
        };
        let (lhs_number, rhs_number) = if string_is_lhs {
            (string_number, NumericValue::Int(int))
        } else {
            (NumericValue::Int(int), string_number)
        };

        if matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul)
            && classified.kind == NumericStringKind::IntString
            && !classified.overflow_or_precision_sensitive
            && let NumericStringValue::Int(string_int) = value
        {
            let (left, right) = if string_is_lhs {
                (string_int, int)
            } else {
                (int, string_int)
            };
            if let Some(result) = checked_int_binary(op, left, right) {
                self.record_counter_numeric_string_specialization_hit();
                return Some(Ok(Value::Int(result)));
            }
        }

        Some(
            execute_arithmetic(op, lhs_number, rhs_number)
                .map_err(|message| Box::new(self.runtime_error(output, compiled, stack, message))),
        )
    }

    /// Emits a cast-coercion warning through the user error handler when one
    /// is installed, mirroring how the reference engine channels these.
    pub(super) fn emit_cast_coercion_warning(
        &self,
        cursor: ExecutionCursor<'_>,
        id: &'static str,
        message: String,
        source_span: RuntimeSourceSpan,
    ) -> Result<(), Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let diagnostic = RuntimeDiagnostic::new(
            id,
            RuntimeSeverity::Warning,
            message,
            source_span,
            Vec::new(),
            Some(php_runtime::api::PhpReferenceClassification::Warning),
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::api::PHP_E_WARNING,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::api::PHP_E_WARNING) {
            Self::record_last_error(state, php_runtime::api::PHP_E_WARNING, &diagnostic);
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::api::PhpDiagnosticChannel::Warning,
                php_runtime::api::PHP_E_WARNING,
            );
            state.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    fn emit_nan_coercion_warning(
        &self,
        cursor: ExecutionCursor<'_>,
        target: &str,
        source_span: RuntimeSourceSpan,
    ) -> Result<(), Box<VmResult>> {
        self.emit_cast_coercion_warning(
            cursor,
            "E_PHP_RUNTIME_NAN_COERCION_WARNING",
            format!("unexpected NAN value was coerced to {target}"),
            source_span,
        )
    }

    pub(super) fn execute_cast(
        &self,
        kind: CastKind,
        src: &Value,
        source_span: RuntimeSourceSpan,
        cursor: ExecutionCursor<'_>,
    ) -> Result<Value, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let float_src_is_nan =
            matches!(effective_value(src), Value::Float(value) if value.to_f64().is_nan());
        match kind {
            CastKind::Bool => {
                if float_src_is_nan {
                    self.emit_nan_coercion_warning(
                        ExecutionCursor::new(compiled, output, stack, state),
                        "bool",
                        source_span.clone(),
                    )?;
                }
                Ok(to_bool(src)
                    .map(Value::Bool)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?)
            }
            CastKind::Int => match src {
                Value::Object(object) => {
                    write_object_numeric_cast_warning(output, state, object, "int", source_span);
                    Ok(Value::Int(1))
                }
                _ => {
                    if let Value::Float(float_value) = effective_value(src) {
                        let raw = float_value.to_f64();
                        if !php_runtime::api::float_fits_int(raw) {
                            let rendered = to_string(&Value::float(raw))
                                .map(|text| text.to_string_lossy())
                                .unwrap_or_else(|_| raw.to_string());
                            self.emit_cast_coercion_warning(
                                ExecutionCursor::new(compiled, output, stack, state),
                                "E_PHP_RUNTIME_FLOAT_TO_INT_CAST_WARNING",
                                format!(
                                    "The float {rendered} is not representable as an int, cast occurred"
                                ),
                                source_span.clone(),
                            )?;
                        }
                    }
                    Ok(to_int(src)
                        .map(Value::Int)
                        .map_err(|message| self.runtime_error(output, compiled, stack, message))?)
                }
            },
            CastKind::Float => match src {
                Value::Object(object) => {
                    write_object_numeric_cast_warning(output, state, object, "float", source_span);
                    Ok(Value::float(1.0))
                }
                _ => Ok(to_float(src)
                    .map(Value::float)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?),
            },
            CastKind::String => {
                if float_src_is_nan {
                    self.emit_nan_coercion_warning(
                        ExecutionCursor::new(compiled, output, stack, state),
                        "string",
                        source_span.clone(),
                    )?;
                }
                self.value_to_string_with_source_span(
                    compiled,
                    src,
                    output,
                    stack,
                    state,
                    source_span,
                )
                .map(Value::String)
            }
            CastKind::Void => Ok(Value::Null),
            CastKind::Array => {
                if float_src_is_nan {
                    self.emit_nan_coercion_warning(
                        ExecutionCursor::new(compiled, output, stack, state),
                        "array",
                        source_span,
                    )?;
                }
                Ok(cast_value_to_array(compiled, stack, src))
            }
            CastKind::Object => {
                if float_src_is_nan {
                    self.emit_nan_coercion_warning(
                        ExecutionCursor::new(compiled, output, stack, state),
                        "object",
                        source_span,
                    )?;
                }
                Ok(cast_value_to_object(src))
            }
        }
    }
}
