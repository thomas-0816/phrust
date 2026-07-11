use super::prelude::*;

impl Vm {
    pub(super) fn value_to_string(
        &self,
        compiled: &CompiledUnit,
        value: &Value,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<PhpString, VmResult> {
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
            other => to_string(other)
                .map_err(|message| self.runtime_error(output, compiled, stack, message)),
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
    ) -> Result<PhpString, VmResult> {
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
            other => to_string(other).map_err(|message| {
                self.runtime_error_with_source_span(output, compiled, stack, source_span, message)
            }),
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
    ) -> Result<String, VmResult> {
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
    ) -> Result<(), VmResult> {
        if state.suppress_array_to_string_warnings > 0 {
            return Ok(());
        }
        let diagnostic = array_to_string_warning(source_span, stack_trace(compiled, stack));
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

    pub(super) fn object_to_string(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<PhpString, VmResult> {
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
    ) -> Result<bool, VmResult> {
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
            Err(message) => Err(self.runtime_error(output, compiled, stack, message)),
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
    ) -> Result<PhpString, VmResult> {
        if is_reflection_runtime_class(&object.class_name()) {
            return reflection_object_to_string(&object)
                .map(|value| PhpString::from(value.into_bytes()))
                .map_err(|message| {
                    self.runtime_error_with_source_span(
                        output,
                        compiled,
                        stack,
                        source_span.clone(),
                        message,
                    )
                });
        }
        if internal_throwable_instanceof(&object.class_name_handle(), "throwable").is_some() {
            return Ok(PhpString::from_bytes(
                throwable_string(&object).into_bytes(),
            ));
        }
        if is_php_token_runtime_class(&object.class_name()) {
            return match object.get_property("text") {
                Some(Value::String(text)) => Ok(text),
                _ => Err(self.runtime_error_with_source_span(
                    output,
                    compiled,
                    stack,
                    source_span,
                    "E_PHP_VM_TOSTRING_RETURN_TYPE: PhpToken::__toString(): Return value must be of type string",
                )),
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
                Ok(other) => Err(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_VM_TOSTRING_RETURN_TYPE: {}::__toString(): Return value must be of type string, {} returned",
                        object.class_name(),
                        value_type_name(&other)
                    ),
                )),
                Err(message) => Err(self.runtime_error(output, compiled, stack, message)),
            };
        }
        if normalize_class_name(&object.class_name()) == "simplexmlelement" {
            return match php_runtime::xml::simplexml_text(&object) {
                Value::String(value) => Ok(value),
                other => Err(self.runtime_error_with_source_span(
                    output,
                    compiled,
                    stack,
                    source_span,
                    format!(
                        "E_PHP_VM_TOSTRING_RETURN_TYPE: SimpleXMLElement::__toString(): Return value must be of type string, {} returned",
                        value_type_name(&other)
                    ),
                )),
            };
        }
        let Some(class) = lookup_class_in_state(compiled, state, &object.class_name()) else {
            return Err(self.runtime_error_with_source_span(
                output,
                compiled,
                stack,
                source_span,
                format!(
                    "E_PHP_VM_UNKNOWN_CLASS: class {} is not defined",
                    object.class_name()
                ),
            ));
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
                return Err(self.runtime_error_with_source_span(
                    output,
                    compiled,
                    stack,
                    source_span,
                    format!(
                        "E_PHP_RUNTIME_OBJECT_TO_STRING_GAP: Object of class {} could not be converted to string",
                        class.display_name
                    ),
                ));
            }
            Err(message) => {
                return Err(self.runtime_error_with_source_span(
                    output,
                    compiled,
                    stack,
                    source_span,
                    message,
                ));
            }
        };
        if resolved.method.flags.is_static
            || resolved.method.flags.is_private
            || resolved.method.flags.is_protected
        {
            return Err(self.runtime_error_with_source_span(
                output,
                compiled,
                stack,
                source_span,
                format!(
                    "E_PHP_VM_TOSTRING_INACCESSIBLE: method {}::__toString is not public instance",
                    resolved.class.name
                ),
            ));
        }
        let result = self.call_object_method_callable(
            compiled,
            object.clone(),
            "__toString",
            Vec::new(),
            None,
            output,
            stack,
            state,
        );
        if !result.status.is_success() {
            return Err(result);
        }
        match result.return_value.unwrap_or(Value::Null) {
            Value::String(value) => Ok(value),
            other => Err(self.runtime_error_with_source_span(
                output,
                compiled,
                stack,
                source_span,
                format!(
                    "E_PHP_VM_TOSTRING_RETURN_TYPE: {}::__toString(): Return value must be of type string, {} returned",
                    class.display_name,
                    value_type_name(&other)
                ),
            )),
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
    ) -> Result<(), VmResult> {
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
    ) -> Result<i32, VmResult> {
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
        compiled: &CompiledUnit,
        op: BinaryOp,
        lhs: &Value,
        rhs: &Value,
        source_span: RuntimeSourceSpan,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
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
                    return Err(unsupported_operand_types_error(
                        output,
                        compiled,
                        stack,
                        lhs,
                        op,
                        rhs,
                        source_span,
                    ));
                }
                if let Some(value) = self.execute_numeric_string_int_binary(
                    compiled,
                    op,
                    lhs,
                    rhs,
                    source_span.clone(),
                    output,
                    stack,
                    state,
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
                execute_arithmetic(op, lhs.value, rhs.value)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))
            }
            BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::ShiftLeft
            | BinaryOp::ShiftRight => execute_bitwise(op, lhs, rhs)
                .map_err(|message| self.runtime_error(output, compiled, stack, message)),
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
                execute_power(lhs, rhs)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))
            }
        }
    }

    pub(super) fn execute_numeric_string_int_binary(
        &self,
        compiled: &CompiledUnit,
        op: BinaryOp,
        lhs: &Value,
        rhs: &Value,
        source_span: RuntimeSourceSpan,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Option<Result<Value, VmResult>> {
        let lhs = effective_value(lhs);
        let rhs = effective_value(rhs);
        let (string, int, string_is_lhs) = match (&lhs, &rhs) {
            (Value::String(string), Value::Int(int)) => (string, *int, true),
            (Value::Int(int), Value::String(string)) => (string, *int, false),
            _ => return None,
        };

        let classified = classify_php_string(string);
        let Some(value) = classified.value else {
            return Some(Err(non_numeric_string_type_error(
                output,
                compiled,
                stack,
                &lhs,
                op,
                &rhs,
                source_span,
            )));
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
                .map_err(|message| self.runtime_error(output, compiled, stack, message)),
        )
    }

    pub(super) fn execute_cast(
        &self,
        compiled: &CompiledUnit,
        kind: CastKind,
        src: &Value,
        source_span: RuntimeSourceSpan,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
        match kind {
            CastKind::Bool => to_bool(src)
                .map(Value::Bool)
                .map_err(|message| self.runtime_error(output, compiled, stack, message)),
            CastKind::Int => match src {
                Value::Object(object) => {
                    write_object_numeric_cast_warning(output, state, object, "int", source_span);
                    Ok(Value::Int(1))
                }
                _ => to_int(src)
                    .map(Value::Int)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message)),
            },
            CastKind::Float => match src {
                Value::Object(object) => {
                    write_object_numeric_cast_warning(output, state, object, "float", source_span);
                    Ok(Value::float(1.0))
                }
                _ => to_float(src)
                    .map(Value::float)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message)),
            },
            CastKind::String => self
                .value_to_string_with_source_span(compiled, src, output, stack, state, source_span)
                .map(Value::String),
            CastKind::Void => Ok(Value::Null),
            CastKind::Array => Ok(cast_value_to_array(compiled, stack, src)),
            CastKind::Object => Ok(cast_value_to_object(src)),
        }
    }
}
