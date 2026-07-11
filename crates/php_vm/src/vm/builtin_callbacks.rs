//! VM-mediated callbacks used by internal builtins.

use super::prelude::*;

impl Vm {
    pub(super) fn execute_json_encode_with_serializable(
        &self,
        entry: BuiltinEntry,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
        call_span: Option<php_ir::IrSpan>,
    ) -> VmResult {
        let flags = json_encode_flags(&values);
        let Some(first) = values.first().cloned() else {
            return execute_builtin_entry(
                entry,
                values,
                output,
                &self.options.runtime_context,
                state,
                builtin_source_span(compiled, call_span),
            );
        };
        let original_first = first.clone();
        let mut transform_state = JsonSerializableEncodeState::default();
        let transformed = match self.prepare_json_serializable_value(
            first,
            flags,
            output,
            stack,
            state,
            compiled,
            &mut transform_state,
        ) {
            Ok(value) => value,
            Err(result) => return result,
        };
        let mut transformed_values = values;
        transformed_values[0] = transformed;
        let mut result = execute_builtin_entry(
            entry,
            transformed_values,
            output,
            &self.options.runtime_context,
            state,
            builtin_source_span(compiled, call_span),
        );
        if result.status.is_success() && transform_state.recursion_error {
            state
                .builtins
                .set_json_last_error(php_runtime::JSON_ERROR_RECURSION);
        }
        if !result.status.is_success() && transform_state.recursion_error {
            result.return_value = Some(Value::Bool(false));
            state
                .builtins
                .set_json_last_error(php_runtime::JSON_ERROR_RECURSION);
        }
        release_unrooted_direct_object_handle(&original_first, stack, state);
        result
    }

    fn prepare_json_serializable_value(
        &self,
        value: Value,
        flags: i64,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
        transform_state: &mut JsonSerializableEncodeState,
    ) -> Result<Value, VmResult> {
        match effective_value(&value) {
            Value::Array(array) => {
                let array_id = array.gc_debug_id();
                if transform_state.active_arrays.contains(&array_id) {
                    if flags & php_runtime::JSON_PARTIAL_OUTPUT_ON_ERROR != 0 {
                        transform_state.recursion_error = true;
                        return Ok(Value::Null);
                    }
                    return Ok(Value::Array(array));
                }
                transform_state.active_arrays.push(array_id);
                let mut transformed = PhpArray::new();
                for (key, element) in array.iter() {
                    let element = match self.prepare_json_serializable_value(
                        element.clone(),
                        flags,
                        output,
                        stack,
                        state,
                        compiled,
                        transform_state,
                    ) {
                        Ok(value) => value,
                        Err(result) => {
                            let _ = transform_state.active_arrays.pop();
                            return Err(result);
                        }
                    };
                    transformed.insert(key, element);
                }
                let _ = transform_state.active_arrays.pop();
                Ok(Value::Array(transformed))
            }
            Value::Object(object) => self.prepare_json_serializable_object(
                object,
                flags,
                output,
                stack,
                state,
                compiled,
                transform_state,
            ),
            Value::Reference(cell) => self.prepare_json_serializable_value(
                cell.get(),
                flags,
                output,
                stack,
                state,
                compiled,
                transform_state,
            ),
            value => Ok(value),
        }
    }

    fn prepare_json_serializable_object(
        &self,
        object: ObjectRef,
        flags: i64,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
        transform_state: &mut JsonSerializableEncodeState,
    ) -> Result<Value, VmResult> {
        if transform_state.active_objects.contains(&object.id())
            || state
                .builtins
                .json_serializable_active_objects
                .contains(&object.id())
        {
            transform_state.recursion_error = true;
            if flags & php_runtime::JSON_PARTIAL_OUTPUT_ON_ERROR != 0 {
                return Ok(Value::Null);
            }
            state
                .builtins
                .set_json_last_error(php_runtime::JSON_ERROR_RECURSION);
            return Err(VmResult::success_no_output(Some(Value::Bool(false))));
        }
        let implements_jsonserializable = match class_implements_in_state(
            compiled,
            state,
            &object.class_name(),
            "JsonSerializable",
            &mut Vec::new(),
        ) {
            Ok(result) => result,
            Err(message) => return Err(self.runtime_error(output, compiled, stack, message)),
        };
        if !implements_jsonserializable {
            return Ok(Value::Object(object));
        }
        transform_state.active_objects.push(object.id());
        state
            .builtins
            .json_serializable_active_objects
            .push(object.id());
        let result = self.call_object_method_callable(
            compiled,
            object.clone(),
            "jsonSerialize",
            Vec::new(),
            None,
            output,
            stack,
            state,
        );
        if !result.status.is_success() {
            let _ = transform_state.active_objects.pop();
            let _ = state.builtins.json_serializable_active_objects.pop();
            return Err(result);
        }
        let serialized = result.return_value.unwrap_or(Value::Null);
        if matches!(effective_value(&serialized), Value::Object(returned) if returned.id() == object.id())
        {
            let _ = transform_state.active_objects.pop();
            let _ = state.builtins.json_serializable_active_objects.pop();
            return Ok(serialized);
        }
        let transformed = self.prepare_json_serializable_value(
            serialized,
            flags,
            output,
            stack,
            state,
            compiled,
            transform_state,
        );
        let _ = transform_state.active_objects.pop();
        let _ = state.builtins.json_serializable_active_objects.pop();
        transformed
    }

    pub(super) fn execute_curl_exec_with_callbacks(
        &self,
        entry: BuiltinEntry,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
        call_span: Option<php_ir::IrSpan>,
    ) -> VmResult {
        let handle = values
            .first()
            .and_then(|value| match effective_value(value) {
                Value::Object(object) => Some(object),
                _ => None,
            });
        let mut result = execute_builtin_entry(
            entry,
            values,
            output,
            &self.options.runtime_context,
            state,
            builtin_source_span(compiled, call_span),
        );
        if !result.status.is_success() {
            return result;
        }
        let Some(handle) = handle else {
            return result;
        };
        let original_return_value = result.return_value.clone();
        let mut diagnostics = std::mem::take(&mut result.diagnostics);
        if let Some(result) = self.call_curl_response_callback(
            compiled,
            &handle,
            "__curl_headerfunction",
            "__curl_last_response_headers",
            output,
            stack,
            state,
            &mut diagnostics,
        ) && !result.status.is_success()
        {
            return result;
        }
        if let Some(result) = self.call_curl_response_callback(
            compiled,
            &handle,
            "__curl_writefunction",
            "__curl_last_response_body",
            output,
            stack,
            state,
            &mut diagnostics,
        ) && !result.status.is_success()
        {
            return result;
        }
        VmResult::success_with_diagnostics_no_output(original_return_value, diagnostics)
    }

    fn call_curl_response_callback(
        &self,
        compiled: &CompiledUnit,
        handle: &ObjectRef,
        callback_property: &str,
        payload_property: &str,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Option<VmResult> {
        let callback = handle.get_property(callback_property)?;
        if !curl_callback_is_enabled(&callback) {
            return None;
        }
        let payload = handle
            .get_property(payload_property)
            .unwrap_or_else(|| Value::string(""));
        let callback_result = self.call_callable_with_by_ref_value_warnings(
            compiled,
            callback,
            vec![
                CallArgument::positional(Value::Object(handle.clone())),
                CallArgument::positional(payload),
            ],
            output,
            stack,
            state,
        );
        diagnostics.extend(callback_result.diagnostics.clone());
        Some(callback_result)
    }

    pub(super) fn execute_xml_parse_with_handlers(
        &self,
        entry: BuiltinEntry,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
        call_span: Option<php_ir::IrSpan>,
    ) -> VmResult {
        let parser = values
            .first()
            .and_then(|value| match effective_value(value) {
                Value::Object(object)
                    if normalize_class_name(&object.class_name()) == "xmlparser" =>
                {
                    Some(object)
                }
                _ => None,
            });
        let input = values
            .get(1)
            .and_then(|value| match effective_value(value) {
                Value::String(input) => Some(input.to_string_lossy()),
                _ => None,
            });
        let mut result = execute_builtin_entry(
            entry,
            values,
            output,
            &self.options.runtime_context,
            state,
            builtin_source_span(compiled, call_span),
        );
        if !result.status.is_success()
            || !matches!(result.return_value.as_ref(), Some(Value::Int(1)))
        {
            return result;
        }
        let (Some(parser), Some(input)) = (parser, input) else {
            return result;
        };
        let Ok(document) = php_runtime::xml::parse_xml(&input) else {
            return result;
        };
        let mut diagnostics = std::mem::take(&mut result.diagnostics);
        let case_folding = xml_parser_case_folding(&parser);
        let context = XmlSaxCallbackContext {
            parser,
            case_folding,
        };
        if let Some(callback_result) = self.dispatch_xml_element_callbacks(
            compiled,
            &context,
            &document.root,
            output,
            stack,
            state,
            &mut diagnostics,
        ) && !callback_result.status.is_success()
        {
            return callback_result;
        }
        VmResult::success_with_diagnostics_no_output(result.return_value, diagnostics)
    }

    fn dispatch_xml_element_callbacks(
        &self,
        compiled: &CompiledUnit,
        context: &XmlSaxCallbackContext,
        element: &php_runtime::xml::XmlElement,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Option<VmResult> {
        let name = xml_sax_name(&element.name, context.case_folding);
        let start_callback = context
            .parser
            .get_property(php_runtime::xml::XML_PARSER_START_ELEMENT_HANDLER);
        if let Some(start_callback) = xml_enabled_callback(start_callback) {
            let callback_result = self.call_callable_with_by_ref_value_warnings(
                compiled,
                start_callback,
                vec![
                    CallArgument::positional(Value::Object(context.parser.clone())),
                    CallArgument::positional(Value::string(name.as_bytes().to_vec())),
                    CallArgument::positional(xml_sax_attributes(element, context.case_folding)),
                ],
                output,
                stack,
                state,
            );
            diagnostics.extend(callback_result.diagnostics.clone());
            if !callback_result.status.is_success() {
                return Some(callback_result);
            }
        } else if let Some(callback_result) = self.dispatch_xml_default_callback(
            compiled,
            context,
            &xml_sax_start_tag(element),
            output,
            stack,
            state,
            diagnostics,
        ) && !callback_result.status.is_success()
        {
            return Some(callback_result);
        }

        for child in &element.children {
            match child {
                php_runtime::xml::XmlNode::Element(child) => {
                    if let Some(callback_result) = self.dispatch_xml_element_callbacks(
                        compiled,
                        context,
                        child,
                        output,
                        stack,
                        state,
                        diagnostics,
                    ) && !callback_result.status.is_success()
                    {
                        return Some(callback_result);
                    }
                }
                php_runtime::xml::XmlNode::Text(text) | php_runtime::xml::XmlNode::Cdata(text) => {
                    if let Some(callback_result) = self.dispatch_xml_character_data_callback(
                        compiled,
                        context,
                        text,
                        output,
                        stack,
                        state,
                        diagnostics,
                    ) && !callback_result.status.is_success()
                    {
                        return Some(callback_result);
                    }
                }
                php_runtime::xml::XmlNode::Comment(text) => {
                    if let Some(callback_result) = self.dispatch_xml_default_callback(
                        compiled,
                        context,
                        &format!("<!--{text}-->"),
                        output,
                        stack,
                        state,
                        diagnostics,
                    ) && !callback_result.status.is_success()
                    {
                        return Some(callback_result);
                    }
                }
            }
        }

        let end_callback = context
            .parser
            .get_property(php_runtime::xml::XML_PARSER_END_ELEMENT_HANDLER);
        if let Some(end_callback) = xml_enabled_callback(end_callback) {
            let callback_result = self.call_callable_with_by_ref_value_warnings(
                compiled,
                end_callback,
                vec![
                    CallArgument::positional(Value::Object(context.parser.clone())),
                    CallArgument::positional(Value::string(name.as_bytes().to_vec())),
                ],
                output,
                stack,
                state,
            );
            diagnostics.extend(callback_result.diagnostics.clone());
            if !callback_result.status.is_success() {
                return Some(callback_result);
            }
        } else if let Some(callback_result) = self.dispatch_xml_default_callback(
            compiled,
            context,
            &format!("</{}>", element.name),
            output,
            stack,
            state,
            diagnostics,
        ) && !callback_result.status.is_success()
        {
            return Some(callback_result);
        }
        None
    }

    fn dispatch_xml_character_data_callback(
        &self,
        compiled: &CompiledUnit,
        context: &XmlSaxCallbackContext,
        text: &str,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Option<VmResult> {
        let callback = context
            .parser
            .get_property(php_runtime::xml::XML_PARSER_CHARACTER_DATA_HANDLER);
        if let Some(callback) = xml_enabled_callback(callback) {
            let callback_result = self.call_callable_with_by_ref_value_warnings(
                compiled,
                callback,
                vec![
                    CallArgument::positional(Value::Object(context.parser.clone())),
                    CallArgument::positional(Value::string(text.as_bytes().to_vec())),
                ],
                output,
                stack,
                state,
            );
            diagnostics.extend(callback_result.diagnostics.clone());
            return Some(callback_result);
        }
        self.dispatch_xml_default_callback(
            compiled,
            context,
            text,
            output,
            stack,
            state,
            diagnostics,
        )
    }

    fn dispatch_xml_default_callback(
        &self,
        compiled: &CompiledUnit,
        context: &XmlSaxCallbackContext,
        data: &str,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Option<VmResult> {
        let callback = context
            .parser
            .get_property(php_runtime::xml::XML_PARSER_DEFAULT_HANDLER);
        let callback = xml_enabled_callback(callback)?;
        let callback_result = self.call_callable_with_by_ref_value_warnings(
            compiled,
            callback,
            vec![
                CallArgument::positional(Value::Object(context.parser.clone())),
                CallArgument::positional(Value::string(data.as_bytes().to_vec())),
            ],
            output,
            stack,
            state,
        );
        diagnostics.extend(callback_result.diagnostics.clone());
        Some(callback_result)
    }

    pub(super) fn prepare_debug_output_values(
        &self,
        builtin: &str,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> Result<Vec<Value>, VmResult> {
        let kind = DebugOutputBuiltin::from_name(builtin);
        values
            .into_iter()
            .map(|value| {
                self.prepare_debug_output_value(kind, value, output, stack, state, compiled)
            })
            .collect()
    }

    fn prepare_debug_output_value(
        &self,
        kind: DebugOutputBuiltin,
        value: Value,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> Result<Value, VmResult> {
        match value {
            Value::Object(object) => self
                .debug_info_object_value(kind, &object, output, stack, state, compiled)
                .map(|debug_value| debug_value.unwrap_or(Value::Object(object))),
            Value::Reference(cell) => {
                let Value::Object(object) = cell.get() else {
                    return Ok(Value::Reference(cell));
                };
                self.debug_info_object_value(kind, &object, output, stack, state, compiled)
                    .map(|debug_value| {
                        debug_value
                            .map(|value| Value::Reference(ReferenceCell::new(value)))
                            .unwrap_or(Value::Reference(cell))
                    })
            }
            value => Ok(value),
        }
    }

    fn debug_info_object_value(
        &self,
        kind: DebugOutputBuiltin,
        object: &ObjectRef,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> Result<Option<Value>, VmResult> {
        let Some(return_value) =
            self.call_debug_info_method(kind, compiled, object, output, stack, state)?
        else {
            return Ok(spl_internal_debug_info_object(object).map(Value::Object));
        };
        let Value::Array(properties) = return_value else {
            return Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_DEBUGINFO_RETURN_TYPE: {}::__debugInfo() must return an array",
                    object.display_name()
                ),
            ));
        };
        Ok(Some(Value::Object(debug_info_object(object, properties))))
    }

    fn call_debug_info_method(
        &self,
        kind: DebugOutputBuiltin,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Option<Value>, VmResult> {
        let Some(_class) = lookup_class_in_state(compiled, state, &object.class_name()) else {
            return Ok(None);
        };
        let resolved = match lookup_resolved_method_in_state(
            compiled,
            state,
            &object.class_name(),
            "__debugInfo",
            None,
        ) {
            Ok(Some(method)) => method,
            Ok(None) => return Ok(None),
            Err(message) => return Err(self.runtime_error(output, compiled, stack, message)),
        };
        if resolved.method.flags.is_static
            || resolved.method.flags.is_private
            || resolved.method.flags.is_protected
        {
            return Ok(None);
        }
        let guard = MagicMethodCall {
            receiver: format!("object:{}", object.id()),
            magic_method: normalize_method_name("__debugInfo"),
            called_method: normalize_method_name(kind.name()),
        };
        if state
            .magic_method_stack
            .iter()
            .any(|active| active == &guard)
        {
            kind.write_recursion(output);
            return Err(VmResult::success(output.clone(), Some(Value::Null)));
        }
        state.magic_method_stack.push(guard);
        let class_owner = class_owner_in_state(compiled, state, &resolved.class.name);
        let result = self.execute_function(
            &class_owner,
            resolved.method.function,
            FunctionCall::new(Vec::new(), Vec::new())
                .with_call_site_strict_types(compiled.unit().strict_types)
                .with_this(object.clone())
                .with_class_context(
                    resolved.class.name.clone(),
                    object.class_name(),
                    resolved.class.name.clone(),
                ),
            output,
            stack,
            state,
        );
        let _ = state.magic_method_stack.pop();
        if !result.status.is_success() {
            return Err(result);
        }
        Ok(Some(result.return_value.unwrap_or(Value::Null)))
    }

    pub(super) fn try_execute_iterator_function(
        &self,
        name: &str,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> Option<VmResult> {
        match name {
            "iterator_apply" => {
                Some(self.execute_iterator_apply(values, output, stack, state, compiled))
            }
            "iterator_count" => {
                Some(self.execute_iterator_count(values, call_span, output, stack, state, compiled))
            }
            "iterator_to_array" => Some(
                self.execute_iterator_to_array(values, call_span, output, stack, state, compiled),
            ),
            _ => None,
        }
    }

    fn execute_iterator_apply(
        &self,
        values: &[Value],
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> VmResult {
        if !(2..=3).contains(&values.len()) {
            let comparator = if values.len() < 2 {
                "at least"
            } else {
                "at most"
            };
            let expected = if values.len() < 2 { 2 } else { 3 };
            let message = format!(
                "iterator_apply() expects {comparator} {expected} arguments, {} given",
                values.len()
            );
            let diagnostic = RuntimeDiagnostic::new(
                "E_PHP_RUNTIME_BUILTIN_ARITY",
                RuntimeSeverity::FatalError,
                message.clone(),
                RuntimeSourceSpan::default(),
                stack_trace(compiled, stack),
                Some(php_runtime::PhpReferenceClassification::Error),
            );
            return VmResult::runtime_error_with_diagnostic(output.clone(), message, diagnostic);
        }
        let callback = values[1].clone();
        let callback_args = match values.get(2).map(effective_value) {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(array)) => call_args_from_php_array(&array),
            Some(other) => {
                let message = format!(
                    "iterator_apply(): Argument #3 ($args) must be of type ?array, {} given",
                    value_type_name(&other)
                );
                let diagnostic = RuntimeDiagnostic::new(
                    "E_PHP_RUNTIME_BUILTIN_TYPE",
                    RuntimeSeverity::FatalError,
                    message.clone(),
                    RuntimeSourceSpan::default(),
                    stack_trace(compiled, stack),
                    Some(php_runtime::PhpReferenceClassification::TypeError),
                );
                return VmResult::runtime_error_with_diagnostic(
                    output.clone(),
                    message,
                    diagnostic,
                );
            }
        };
        if let Err(error) = validate_array_callback_arg(
            compiled,
            state,
            "iterator_apply",
            2,
            "callback",
            false,
            &callback,
        ) {
            return match error {
                ArrayCallbackError::Runtime(result) => *result,
                ArrayCallbackError::BuiltinType { function, actual } => {
                    array_callback_type_error(output, compiled, stack, function, &actual)
                }
                ArrayCallbackError::Message(message) => {
                    self.runtime_error(output, compiled, stack, message)
                }
            };
        }
        let source = effective_value(&values[0]);
        match iterator_function_accepts_source(compiled, state, &source) {
            Ok(true) => {}
            Ok(false) => {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_RUNTIME_BUILTIN_TYPE: iterator_count(): Argument #1 ($iterator) must be of type Traversable|array, {} given",
                        type_error_value_name(&source)
                    ),
                );
            }
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        }
        let mut iterator = match self.foreach_iterator_from_value(
            compiled,
            source,
            output,
            stack,
            state,
            ForeachInvalidSourceBehavior::Unsupported,
        ) {
            Ok(iterator) => {
                let mut iterators = HashMap::new();
                iterators.insert(RegId::new(0), iterator);
                iterators
            }
            Err(result) => return result,
        };
        let mut count = 0_i64;
        loop {
            match self.next_foreach_value(
                compiled,
                output,
                stack,
                state,
                &mut iterator,
                RegId::new(0),
                false,
            ) {
                Ok(Some(_)) => {
                    let callback_result = self.call_callable_with_by_ref_value_warnings(
                        compiled,
                        callback.clone(),
                        callback_args.clone(),
                        output,
                        stack,
                        state,
                    );
                    if !callback_result.status.is_success() {
                        return callback_result;
                    }
                    count += 1;
                    let should_continue = callback_result
                        .return_value
                        .as_ref()
                        .is_some_and(|value| to_bool(value).unwrap_or(false));
                    if !should_continue {
                        return VmResult::success_no_output(Some(Value::Int(count)));
                    }
                }
                Ok(None) => return VmResult::success_no_output(Some(Value::Int(count))),
                Err(result) => {
                    self.annotate_iterator_builtin_iteration_failure(
                        &result,
                        "iterator_apply",
                        values,
                        None,
                        compiled,
                        stack,
                        state,
                    );
                    return result;
                }
            }
        }
    }

    fn execute_iterator_count(
        &self,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> VmResult {
        if values.len() != 1 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_ITERATOR_COUNT_ARITY: iterator_count expects exactly 1 argument, {} given",
                    values.len()
                ),
            );
        }
        let source = effective_value(&values[0]);
        if let Err(result) = self.validate_iterator_function_iterable_arg(
            "iterator_count",
            values,
            source.clone(),
            call_span,
            output,
            stack,
            state,
            compiled,
        ) {
            return result;
        }
        let mut iterator = match self.foreach_iterator_from_value(
            compiled,
            source,
            output,
            stack,
            state,
            ForeachInvalidSourceBehavior::Unsupported,
        ) {
            Ok(iterator) => {
                let mut iterators = HashMap::new();
                iterators.insert(RegId::new(0), iterator);
                iterators
            }
            Err(result) => return result,
        };
        let mut count = 0_i64;
        loop {
            match self.next_foreach_value(
                compiled,
                output,
                stack,
                state,
                &mut iterator,
                RegId::new(0),
                false,
            ) {
                Ok(Some(_)) => count += 1,
                Ok(None) => return VmResult::success_no_output(Some(Value::Int(count))),
                Err(result) => {
                    self.annotate_iterator_builtin_iteration_failure(
                        &result,
                        "iterator_count",
                        values,
                        call_span,
                        compiled,
                        stack,
                        state,
                    );
                    return result;
                }
            }
        }
    }

    fn execute_iterator_to_array(
        &self,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> VmResult {
        if !(1..=2).contains(&values.len()) {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_ITERATOR_TO_ARRAY_ARITY: iterator_to_array expects 1 or 2 arguments, {} given",
                    values.len()
                ),
            );
        }
        if let Err(result) = self.validate_iterator_function_iterable_arg(
            "iterator_to_array",
            values,
            effective_value(&values[0]),
            call_span,
            output,
            stack,
            state,
            compiled,
        ) {
            return result;
        }
        let preserve_keys = match values.get(1) {
            Some(value) => match to_bool(value) {
                Ok(value) => value,
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            },
            None => true,
        };
        let source = effective_value(&values[0]);
        match iterator_function_accepts_source(compiled, state, &source) {
            Ok(true) => {}
            Ok(false) => {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_RUNTIME_BUILTIN_TYPE: iterator_to_array(): Argument #1 ($iterator) must be of type Traversable|array, {} given",
                        type_error_value_name(&source)
                    ),
                );
            }
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        }
        let mut iterator = match self.foreach_iterator_from_value(
            compiled,
            source,
            output,
            stack,
            state,
            ForeachInvalidSourceBehavior::Unsupported,
        ) {
            Ok(iterator) => {
                let mut iterators = HashMap::new();
                iterators.insert(RegId::new(0), iterator);
                iterators
            }
            Err(result) => return result,
        };
        let mut result = PhpArray::new();
        loop {
            match self.next_foreach_value(
                compiled,
                output,
                stack,
                state,
                &mut iterator,
                RegId::new(0),
                true,
            ) {
                Ok(Some((key, value))) => {
                    if preserve_keys {
                        let Some(key) = key else {
                            result.append(value);
                            continue;
                        };
                        let key = match self.iterator_to_array_preserved_key(
                            &key, call_span, output, stack, state, compiled,
                        ) {
                            Ok(key) => key,
                            Err(result) => return result,
                        };
                        result.insert(key, value);
                    } else {
                        result.append(value);
                    }
                }
                Ok(None) => return VmResult::success_no_output(Some(Value::Array(result))),
                Err(result) => {
                    self.annotate_iterator_builtin_iteration_failure(
                        &result,
                        "iterator_to_array",
                        values,
                        call_span,
                        compiled,
                        stack,
                        state,
                    );
                    return result;
                }
            }
        }
    }

    fn iterator_to_array_preserved_key(
        &self,
        key: &Value,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> Result<ArrayKey, VmResult> {
        match effective_value(key) {
            Value::Float(float) => {
                let number = float.to_f64();
                if number.is_finite() && number.fract() != 0.0 {
                    emit_iterator_to_array_key_deprecation(
                        output,
                        stack,
                        state,
                        compiled,
                        call_span,
                        "E_PHP_VM_ITERATOR_TO_ARRAY_FLOAT_KEY_DEPRECATED",
                        format!("Implicit conversion from float {float} to int loses precision"),
                    );
                }
                Ok(ArrayKey::Int(number as i64))
            }
            Value::Null => {
                emit_iterator_to_array_key_deprecation(
                    output,
                    stack,
                    state,
                    compiled,
                    call_span,
                    "E_PHP_VM_ITERATOR_TO_ARRAY_NULL_KEY_DEPRECATED",
                    "Using null as an array offset is deprecated, use an empty string instead"
                        .to_owned(),
                );
                Ok(ArrayKey::String(PhpString::from_bytes(Vec::new())))
            }
            Value::Array(_) => Err(self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_ARRAY_KEY_CONVERSION: Cannot access offset of type array on array",
            )),
            other => array_key_from_value(&other)
                .map_err(|message| self.runtime_error(output, compiled, stack, message)),
        }
    }

    fn annotate_iterator_builtin_iteration_failure(
        &self,
        result: &VmResult,
        function: &str,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        compiled: &CompiledUnit,
        stack: &CallStack,
        state: &mut ExecutionState,
    ) {
        let Some(call_span) = call_span else {
            return;
        };
        let Some(throwable) = state
            .pending_throw
            .clone()
            .or_else(|| runtime_error_throwable(result))
        else {
            return;
        };
        append_throwable_internal_iterator_trace_arg_frame(
            &throwable, compiled, function, values, call_span,
        );
        state.pending_trace = Some(
            capture_backtrace_string_with_internal_iterator_builtin_call(
                compiled, stack, function, values, call_span,
            ),
        );
        state.pending_throw = Some(throwable);
    }

    fn validate_iterator_function_iterable_arg(
        &self,
        function: &str,
        values: &[Value],
        value: Value,
        call_span: Option<php_ir::IrSpan>,
        output: &OutputBuffer,
        stack: &CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> Result<(), VmResult> {
        if iterator_function_accepts_iterable(compiled, state, &value)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?
        {
            return Ok(());
        }
        let message = format!(
            "{function}(): Argument #1 ($iterator) must be of type Traversable|array, {} given",
            type_error_value_name(&value)
        );
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            RuntimeSeverity::FatalError,
            message.clone(),
            RuntimeSourceSpan::default(),
            stack_trace(compiled, stack),
            Some(php_runtime::PhpReferenceClassification::TypeError),
        );
        if let Some(call_span) = call_span {
            let result = VmResult::runtime_error_with_diagnostic(
                output.clone(),
                message.clone(),
                diagnostic.clone(),
            );
            if let Some(throwable) = runtime_error_throwable(&result) {
                tag_throwable_location(&throwable, compiled, call_span);
                state.pending_trace = Some(capture_backtrace_string_with_builtin_failed_call(
                    compiled, stack, function, values, call_span,
                ));
                state.pending_throw = Some(throwable);
                return Err(result);
            }
        }
        Err(VmResult::runtime_error_with_diagnostic(
            output.clone(),
            message,
            diagnostic,
        ))
    }
}

fn emit_iterator_to_array_key_deprecation(
    output: &mut OutputBuffer,
    stack: &CallStack,
    state: &mut ExecutionState,
    compiled: &CompiledUnit,
    call_span: Option<php_ir::IrSpan>,
    id: &'static str,
    message: String,
) {
    let diagnostic = RuntimeDiagnostic::new(
        id,
        RuntimeSeverity::Deprecation,
        message,
        builtin_source_span(compiled, call_span),
        stack_trace(compiled, stack),
        None,
    );
    emit_vm_diagnostic(
        output,
        state,
        &diagnostic,
        php_runtime::PhpDiagnosticChannel::Deprecated,
        php_runtime::PHP_E_DEPRECATED,
    );
    state.diagnostics.push(diagnostic);
}

#[derive(Clone)]
struct XmlSaxCallbackContext {
    parser: ObjectRef,
    case_folding: bool,
}

fn xml_parser_case_folding(parser: &ObjectRef) -> bool {
    match parser.get_property("__phrust_xml_case_folding") {
        Some(Value::Bool(enabled)) => enabled,
        Some(value) => to_bool(&value).unwrap_or(true),
        None => true,
    }
}

fn xml_enabled_callback(callback: Option<Value>) -> Option<Value> {
    callback.filter(|value| !matches!(value, Value::Null))
}

fn xml_sax_name(name: &str, case_folding: bool) -> String {
    if case_folding {
        name.to_ascii_uppercase()
    } else {
        name.to_owned()
    }
}

fn xml_sax_attributes(element: &php_runtime::xml::XmlElement, case_folding: bool) -> Value {
    let mut attributes = PhpArray::new();
    for (name, value) in &element.attributes {
        let key = xml_sax_name(name, case_folding);
        attributes.insert(
            ArrayKey::String(PhpString::from_bytes(key.into_bytes())),
            Value::string(value.as_bytes().to_vec()),
        );
    }
    Value::Array(attributes)
}

fn xml_sax_start_tag(element: &php_runtime::xml::XmlElement) -> String {
    let mut tag = String::new();
    tag.push('<');
    tag.push_str(&element.name);
    for (name, value) in &element.attributes {
        tag.push(' ');
        tag.push_str(name);
        tag.push_str("=\"");
        tag.push_str(&xml_sax_escape_attribute(value));
        tag.push('"');
    }
    tag.push('>');
    tag
}

fn xml_sax_escape_attribute(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '"' => escaped.push_str("&quot;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[derive(Default)]
struct JsonSerializableEncodeState {
    active_arrays: Vec<usize>,
    active_objects: Vec<u64>,
    recursion_error: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DebugOutputBuiltin {
    VarDump,
    PrintR,
}

impl DebugOutputBuiltin {
    fn from_name(name: &str) -> Self {
        match name {
            "print_r" => Self::PrintR,
            _ => Self::VarDump,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::VarDump => "var_dump",
            Self::PrintR => "print_r",
        }
    }

    fn write_recursion(self, output: &mut OutputBuffer) {
        match self {
            Self::VarDump => output.write_test_str("*RECURSION*\n"),
            Self::PrintR => output.write_test_str("*RECURSION*"),
        }
    }
}

fn json_encode_flags(values: &[Value]) -> i64 {
    match values.get(1).map(effective_value) {
        Some(Value::Int(flags)) => flags,
        _ => 0,
    }
}
