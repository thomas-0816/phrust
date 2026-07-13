use super::prelude::*;

impl Vm {
    pub(super) fn try_execute_serialization_builtin(
        &self,
        cursor: ExecutionCursor<'_>,
        name: &str,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
    ) -> Option<VmResult> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        match name {
            "serialize" => self.try_execute_serialize_with_magic(
                compiled, values, call_span, output, stack, state,
            ),
            "unserialize" => {
                self.try_execute_unserialize_with_autoload(compiled, values, output, stack, state)
            }
            _ => None,
        }
    }

    pub(super) fn try_execute_serialize_with_magic(
        &self,
        compiled: &CompiledUnit,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Option<VmResult> {
        if values.len() != 1 {
            return None;
        }
        let Value::Object(object) = effective_value(&values[0]) else {
            return None;
        };
        let result =
            self.serialize_object_with_magic(compiled, object, call_span, output, stack, state);
        Some(match result {
            Ok(value) => VmResult::success(OutputBuffer::new(), Some(Value::String(value))),
            Err(result) => {
                if let Some(throwable) = runtime_error_throwable(&result) {
                    if let Some(call_span) = call_span {
                        tag_throwable_location(&throwable, compiled, call_span);
                        reapply_throwable_diagnostic_overrides(&throwable, &result);
                        state.pending_trace = Some(attach_builtin_failed_call_trace(
                            &throwable,
                            compiled,
                            stack,
                            "serialize",
                            values,
                            call_span,
                        ));
                    } else {
                        state.pending_trace = Some(capture_backtrace_string(compiled, stack));
                    }
                    state.pending_throw = Some(throwable);
                }
                *result
            }
        })
    }

    pub(super) fn try_execute_unserialize_with_autoload(
        &self,
        compiled: &CompiledUnit,
        values: &[Value],
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Option<VmResult> {
        if !(1..=2).contains(&values.len()) {
            return None;
        }
        let Value::String(input) = effective_value(&values[0]) else {
            return None;
        };
        if let Some(custom) = parse_legacy_serializable_payload(&input) {
            let result =
                self.unserialize_legacy_serializable(compiled, custom, output, stack, state);
            return Some(match result {
                Ok(value) => VmResult::success(OutputBuffer::new(), Some(value)),
                Err(result) => *result,
            });
        }
        if let Some(message) = self.spl_unserialize_payload_error(compiled, state, &input) {
            return Some(self.throw_catchable_exception(compiled, output, stack, state, message));
        }
        let value = match unserialize_value(&input, UnserializeOptions::default()) {
            Ok(value) => value,
            Err(_) => return None,
        };
        let result = self.resolve_unserialized_classes(compiled, value, output, stack, state);
        Some(match result {
            Ok(value) => VmResult::success(OutputBuffer::new(), Some(value)),
            Err(result) => *result,
        })
    }

    pub(super) fn resolve_unserialized_classes(
        &self,
        compiled: &CompiledUnit,
        value: Value,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, Box<VmResult>> {
        match value {
            Value::Object(object) => {
                let class_name = object.display_name();
                if class_like_exists_direct(
                    compiled,
                    state,
                    &class_name,
                    AutoloadClassLookupKind::Class,
                ) {
                    return Ok(Value::Object(object));
                }
                self.autoload_class(compiled, &class_name, output, stack, state, None)?;
                if class_like_exists_direct(
                    compiled,
                    state,
                    &class_name,
                    AutoloadClassLookupKind::Class,
                ) {
                    Ok(Value::Object(object))
                } else {
                    Ok(Value::Object(incomplete_class_object(class_name, object)))
                }
            }
            Value::Array(array) => {
                let mut resolved = PhpArray::new();
                for (key, element) in array.iter() {
                    let element = self.resolve_unserialized_classes(
                        compiled,
                        element.clone(),
                        output,
                        stack,
                        state,
                    )?;
                    resolved.insert(key.clone(), element);
                }
                Ok(Value::Array(resolved))
            }
            Value::Reference(cell) => {
                let resolved =
                    self.resolve_unserialized_classes(compiled, cell.get(), output, stack, state)?;
                cell.set(resolved);
                Ok(Value::Reference(cell))
            }
            other => Ok(other),
        }
    }

    pub(super) fn spl_unserialize_payload_error(
        &self,
        compiled: &CompiledUnit,
        state: &ExecutionState,
        input: &PhpString,
    ) -> Option<String> {
        let (class_name, payload) = parse_indexed_serialized_object_payload(input)?;
        let normalized = normalize_class_name(&class_name);
        if normalized == "hashcontext" {
            return validate_hash_context_unserialize_payload(&payload);
        }
        match normalized.as_str() {
            "arrayobject" | "arrayiterator" => validate_spl_array_container_unserialize_payload(
                compiled,
                state,
                &class_name,
                &payload,
            ),
            "spldoublylinkedlist" => validate_spl_doubly_linked_list_unserialize_payload(&payload),
            "splobjectstorage" => validate_spl_object_storage_unserialize_payload(&payload),
            _ => None,
        }
    }

    pub(super) fn vm_serialize_error_message(message: &str) -> String {
        if message == "Serialization of 'XMLParser' is not allowed" {
            format!("E_PHP_VM_EXCEPTION: {message}")
        } else if message == "HashContext with HASH_HMAC option cannot be serialized"
            || (message.starts_with("HashContext for algorithm \"")
                && message.ends_with("\" cannot be serialized"))
        {
            format!("E_PHP_VM_SPL_RUNTIME_EXCEPTION: {message}")
        } else {
            format!("E_PHP_VM_SERIALIZE_ERROR: {message}")
        }
    }

    pub(super) fn serialize_object_with_magic(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<PhpString, Box<VmResult>> {
        let Some(class) = lookup_class_in_state(compiled, state, &object.class_name()) else {
            return Ok(serialize_value(&Value::Object(object)).map_err(|error| {
                self.runtime_error(
                    output,
                    compiled,
                    stack,
                    Self::vm_serialize_error_message(error.message()),
                )
            })?);
        };
        if class_implements_in_state(
            compiled,
            state,
            &class.name,
            "Serializable",
            &mut Vec::new(),
        )
        .map_err(|message| self.runtime_error(output, compiled, stack, message))?
        {
            return self.serialize_legacy_serializable(
                ExecutionCursor::new(compiled, output, stack, state),
                object,
                &class,
                call_span,
            );
        }
        let serialize_method =
            match lookup_method_in_hierarchy(compiled, &class, "__serialize", None) {
                Ok(method) => method,
                Err(message) => {
                    return Err(Box::new(
                        self.runtime_error(output, compiled, stack, message),
                    ));
                }
            };
        if let Some(resolved) = serialize_method {
            if resolved.method.flags.is_static {
                return Err(Box::new(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_VM_SLEEP_METHOD_INACCESSIBLE: method {}::__serialize is not public instance",
                        resolved.class.name
                    ),
                )));
            }
            let owner = class_owner_in_state(compiled, state, &resolved.class.name);
            let result = self.execute_function(
                &owner,
                resolved.method.function,
                FunctionCall::new(Vec::new(), Vec::new())
                    .with_call_site_strict_types(owner.unit().strict_types)
                    .with_this(object.clone())
                    .with_class_context_handles(
                        self.class_name_handles(&resolved.class.name).normalized,
                        object_called_class_handle(&object),
                        self.class_name_handles(&resolved.class.name).normalized,
                    )
                    .with_optional_call_span(call_span),
                output,
                stack,
                state,
            );
            if !result.status.is_success() {
                return Err(Box::new(result));
            }
            let Value::Array(properties) =
                effective_value(&result.return_value.unwrap_or(Value::Null))
            else {
                return Err(Box::new(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_VM_SLEEP_RETURN_TYPE: {}::__serialize() must return an array",
                        class.display_name
                    ),
                )));
            };
            let runtime_class = runtime_class_entry(
                compiled,
                state,
                &class,
                &|value| self.constant_value(compiled.unit(), value),
                &|reference| class_constant_reference_value(compiled, state, reference),
                &|reference| named_constant_reference_value(compiled, state, reference),
            )
            .map_err(|error| self.runtime_error(output, compiled, stack, error.into_message()))?;
            let filtered = ObjectRef::new_with_display_name(&runtime_class, object.display_name());
            for (storage_name, _) in filtered.properties_snapshot() {
                filtered.unset_property(&storage_name);
            }
            for (key, value) in properties.iter() {
                let name = match key {
                    ArrayKey::String(name) => name.to_string_lossy(),
                    ArrayKey::Int(index) => index.to_string(),
                };
                filtered.set_property(name, effective_value(value));
            }
            return Ok(serialize_value(&Value::Object(filtered)).map_err(|error| {
                self.runtime_error(
                    output,
                    compiled,
                    stack,
                    Self::vm_serialize_error_message(error.message()),
                )
            })?);
        }
        let resolved = match lookup_method_in_hierarchy(compiled, &class, "__sleep", None) {
            Ok(Some(method)) => method,
            Ok(None) => {
                return Ok(serialize_value(&Value::Object(object)).map_err(|error| {
                    self.runtime_error(
                        output,
                        compiled,
                        stack,
                        Self::vm_serialize_error_message(error.message()),
                    )
                })?);
            }
            Err(message) => {
                return Err(Box::new(
                    self.runtime_error(output, compiled, stack, message),
                ));
            }
        };
        if resolved.method.flags.is_static {
            return Err(Box::new(self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_SLEEP_METHOD_INACCESSIBLE: method {}::__sleep is not public instance",
                    resolved.class.name
                ),
            )));
        }
        let owner = class_owner_in_state(compiled, state, &resolved.class.name);
        let result = self.execute_function(
            &owner,
            resolved.method.function,
            FunctionCall::new(Vec::new(), Vec::new())
                .with_call_site_strict_types(owner.unit().strict_types)
                .with_this(object.clone())
                .with_class_context_handles(
                    self.class_name_handles(&resolved.class.name).normalized,
                    object_called_class_handle(&object),
                    self.class_name_handles(&resolved.class.name).normalized,
                )
                .with_optional_call_span(call_span),
            output,
            stack,
            state,
        );
        if !result.status.is_success() {
            return Err(Box::new(result));
        }
        let Value::Array(selected) = effective_value(&result.return_value.unwrap_or(Value::Null))
        else {
            return Err(Box::new(self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_SLEEP_RETURN_TYPE: {}::__sleep(): Return value must be of type array",
                    class.display_name
                ),
            )));
        };
        let runtime_class = runtime_class_entry(
            compiled,
            state,
            &class,
            &|value| self.constant_value(compiled.unit(), value),
            &|reference| class_constant_reference_value(compiled, state, reference),
            &|reference| named_constant_reference_value(compiled, state, reference),
        )
        .map_err(|error| self.runtime_error(output, compiled, stack, error.into_message()))?;
        let filtered = ObjectRef::new_with_display_name(&runtime_class, object.display_name());
        for (storage_name, _) in filtered.properties_snapshot() {
            filtered.unset_property(&storage_name);
        }
        let source_properties = object.properties_snapshot();
        for (_, selected_name) in selected.iter() {
            let Value::String(selected_name) = effective_value(selected_name) else {
                continue;
            };
            let selected_name = selected_name.to_string_lossy();
            let Some((storage_name, value)) =
                sleep_property_value(&source_properties, &selected_name)
            else {
                self.emit_serialize_sleep_missing_property_warning(
                    compiled,
                    output,
                    stack,
                    state,
                    &selected_name,
                    call_span,
                )?;
                continue;
            };
            filtered.set_property(storage_name, effective_value(&value));
        }
        Ok(serialize_value(&Value::Object(filtered)).map_err(|error| {
            self.runtime_error(
                output,
                compiled,
                stack,
                Self::vm_serialize_error_message(error.message()),
            )
        })?)
    }

    pub(super) fn call_spl_container_method_with_magic(
        &self,
        cursor: ExecutionCursor<'_>,
        object: ObjectRef,
        method: &str,
        args: Vec<CallArgument>,
        call_span: Option<IrSpan>,
    ) -> Result<Value, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let normalized_method = normalize_method_name(method);
        if normalized_method == "serialize" {
            let runtime_class = spl_runtime_marker(&object)
                .unwrap_or_else(|| normalize_class_name(&object.class_name()));
            if matches!(
                runtime_class.as_str(),
                "spldoublylinkedlist" | "splstack" | "splqueue" | "splobjectstorage"
            ) {
                validate_spl_iterator_arg_count(&object.class_name(), &args, 0, 0)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                let bytes = self.serialize_spl_container_legacy(
                    ExecutionCursor::new(compiled, output, stack, state),
                    &object,
                    &runtime_class,
                    call_span,
                )?;
                return Ok(Value::String(bytes));
            }
        }
        let replaced_storage_info = if spl_runtime_marker(&object).as_deref()
            == Some("splobjectstorage")
            && normalized_method == "setinfo"
        {
            let pos = spl_position(&object);
            spl_storage_entries(&object)
                .get(pos)
                .map(|(_, _, info)| info.clone())
        } else {
            None
        };
        let value = call_spl_container_method(object, method, args)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        if let Some(info) = replaced_storage_info {
            let candidates = destructor_candidates_for_value(&info);
            if !candidates.is_empty() {
                let rooted_object_ids = php_visible_non_register_root_object_ids(stack, state);
                let mut handlers = Vec::new();
                let mut pending_control = None;
                let sweep = self.run_destructors_for_unreferenced_candidates_with_roots(
                    ExecutionCursor::new(compiled, output, stack, state),
                    &mut handlers,
                    &mut pending_control,
                    candidates,
                    &rooted_object_ids,
                    None,
                );
                if let Some(outcome) = sweep.outcome {
                    match outcome {
                        RaiseOutcome::Caught(_) => {}
                        RaiseOutcome::Done(result) => return Err(Box::new(*result)),
                    }
                }
            }
        }
        Ok(value)
    }

    pub(super) fn serialize_spl_container_legacy(
        &self,
        cursor: ExecutionCursor<'_>,
        object: &ObjectRef,
        runtime_class: &str,
        call_span: Option<IrSpan>,
    ) -> Result<PhpString, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let mut bytes = Vec::new();
        if normalize_class_name(runtime_class) == "splobjectstorage" {
            let records = spl_storage_entries(object);
            bytes.extend_from_slice(format!("x:i:{};", records.len()).as_bytes());
            for (_, item, info) in records {
                let item = self.serialize_value_with_magic(
                    compiled,
                    Value::Object(item),
                    call_span,
                    output,
                    stack,
                    state,
                )?;
                bytes.extend_from_slice(item.as_bytes());
                bytes.push(b',');
                let info = self
                    .serialize_value_with_magic(compiled, info, call_span, output, stack, state)?;
                bytes.extend_from_slice(info.as_bytes());
                bytes.push(b';');
            }
            bytes.extend_from_slice(b"m:");
            let properties = self.serialize_value_with_magic(
                compiled,
                Value::Array(spl_object_user_properties_array(object)),
                call_span,
                output,
                stack,
                state,
            )?;
            bytes.extend_from_slice(properties.as_bytes());
            return Ok(PhpString::from(bytes));
        }

        let mut index = 0usize;
        loop {
            let entries = spl_entries(object);
            let Some((key, value)) = entries.get(index).cloned() else {
                break;
            };
            let key = self.serialize_value_with_magic(
                compiled,
                array_key_to_value(key),
                call_span,
                output,
                stack,
                state,
            )?;
            bytes.extend_from_slice(key.as_bytes());
            bytes.push(b':');
            let value =
                self.serialize_value_with_magic(compiled, value, call_span, output, stack, state)?;
            bytes.extend_from_slice(value.as_bytes());
            index += 1;
        }
        Ok(PhpString::from(bytes))
    }

    pub(super) fn serialize_value_with_magic(
        &self,
        compiled: &CompiledUnit,
        value: Value,
        call_span: Option<IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<PhpString, Box<VmResult>> {
        match effective_value(&value) {
            Value::Object(object) => {
                self.serialize_object_with_magic(compiled, object, call_span, output, stack, state)
            }
            other => Ok(serialize_value(&other).map_err(|error| {
                self.runtime_error(
                    output,
                    compiled,
                    stack,
                    Self::vm_serialize_error_message(error.message()),
                )
            })?),
        }
    }

    pub(super) fn call_hash_context_method(
        &self,
        object: &ObjectRef,
        method: &str,
        args: &[CallArgument],
    ) -> Result<Value, String> {
        match normalize_method_name(method).as_str() {
            "__debuginfo" => {
                if !args.is_empty() {
                    return Err(format!(
                        "E_PHP_VM_TOO_MANY_ARGS: HashContext::__debugInfo() expects exactly 0 arguments, {} given",
                        args.len()
                    ));
                }
                let Some(properties) = hash_context_debug_info_array(object) else {
                    return Err(
                        "E_PHP_VM_INVALID_HASH_CONTEXT: invalid HashContext state".to_owned()
                    );
                };
                Ok(Value::Array(properties))
            }
            "__serialize" => {
                validate_hash_context_arg_count("__serialize", args, 0)?;
                hash_context_serialize_array(object).map(Value::Array)
            }
            "__unserialize" => {
                validate_hash_context_arg_count("__unserialize", args, 1)?;
                if hash_context_object_is_initialized(object) {
                    return Err(hash_context_runtime_exception(
                        "HashContext::__unserialize called on initialized object",
                    ));
                }
                let Value::Array(payload) = effective_value(&args[0].value) else {
                    return Err(format!(
                        "E_PHP_VM_TYPE_ERROR: HashContext::__unserialize(): Argument #1 ($data) must be of type array, {} given",
                        value_type_name(&args[0].value)
                    ));
                };
                if let Some(message) = validate_hash_context_unserialize_payload(&payload) {
                    return Err(hash_context_runtime_exception(message));
                }
                Err(hash_context_runtime_exception(
                    "Incomplete or ill-formed serialization data",
                ))
            }
            _ => Err(format!(
                "E_PHP_VM_METHOD_NOT_FOUND: Call to undefined method HashContext::{method}()"
            )),
        }
    }

    pub(super) fn serialize_legacy_serializable(
        &self,
        cursor: ExecutionCursor<'_>,
        object: ObjectRef,
        class: &php_ir::module::ClassEntry,
        call_span: Option<php_ir::IrSpan>,
    ) -> Result<PhpString, Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let resolved = match lookup_method_in_hierarchy(compiled, class, "serialize", None) {
            Ok(Some(method)) => method,
            Ok(None) => {
                return Ok(serialize_value(&Value::Object(object)).map_err(|error| {
                    self.runtime_error(
                        output,
                        compiled,
                        stack,
                        Self::vm_serialize_error_message(error.message()),
                    )
                })?);
            }
            Err(message) => {
                return Err(Box::new(
                    self.runtime_error(output, compiled, stack, message),
                ));
            }
        };
        if resolved.method.flags.is_static {
            return Err(Box::new(self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_SERIALIZABLE_METHOD_INACCESSIBLE: method {}::serialize is not public instance",
                    resolved.class.name
                ),
            )));
        }
        let result = self.execute_function(
            compiled,
            resolved.method.function,
            FunctionCall::new(Vec::new(), Vec::new())
                .with_this(object)
                .with_class_context(
                    resolved.class.name.clone(),
                    class.name.clone(),
                    resolved.class.name.clone(),
                )
                .with_optional_call_span(call_span),
            output,
            stack,
            state,
        );
        if !result.status.is_success() {
            return Err(Box::new(result));
        }
        match effective_value(&result.return_value.unwrap_or(Value::Null)) {
            Value::String(payload) => Ok(legacy_serializable_wire(&class.display_name, &payload)),
            Value::Null => Ok(PhpString::from_test_str("N;")),
            _ => Err(Box::new(self.throw_exception_result(
                compiled,
                output,
                stack,
                state,
                call_span.unwrap_or_default(),
                format!(
                    "{}::serialize() must return a string or NULL",
                    class.display_name
                ),
            ))),
        }
    }

    pub(super) fn unserialize_legacy_serializable(
        &self,
        compiled: &CompiledUnit,
        payload: LegacySerializablePayload,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, Box<VmResult>> {
        self.autoload_class(compiled, &payload.class_name, output, stack, state, None)?;
        let Some(class) = lookup_class_in_state(compiled, state, &payload.class_name) else {
            let source = ObjectRef::new_with_display_name(
                &empty_runtime_class(&payload.class_name),
                payload.class_name.clone(),
            );
            return Ok(Value::Object(incomplete_class_object(
                source.display_name(),
                source,
            )));
        };
        let runtime_class = runtime_class_entry(
            compiled,
            state,
            &class,
            &|value| self.constant_value(compiled.unit(), value),
            &|reference| class_constant_reference_value(compiled, state, reference),
            &|reference| named_constant_reference_value(compiled, state, reference),
        )
        .map_err(|error| self.runtime_error(output, compiled, stack, error.into_message()))?;
        let object = ObjectRef::new_with_display_name(&runtime_class, class.display_name.clone());
        let resolved = match lookup_method_in_hierarchy(compiled, &class, "unserialize", None) {
            Ok(Some(method)) => method,
            Ok(None) => return Ok(Value::Object(object)),
            Err(message) => {
                return Err(Box::new(
                    self.runtime_error(output, compiled, stack, message),
                ));
            }
        };
        if resolved.method.flags.is_static {
            return Err(Box::new(self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_SERIALIZABLE_METHOD_INACCESSIBLE: method {}::unserialize is not public instance",
                    resolved.class.name
                ),
            )));
        }
        let result = self.execute_function(
            compiled,
            resolved.method.function,
            FunctionCall::new(
                vec![CallArgument::positional(Value::String(payload.payload))],
                Vec::new(),
            )
            .with_this(object.clone())
            .with_class_context(
                resolved.class.name.clone(),
                class.name.clone(),
                resolved.class.name.clone(),
            ),
            output,
            stack,
            state,
        );
        if !result.status.is_success() {
            return Err(Box::new(result));
        }
        Ok(Value::Object(object))
    }

    pub(super) fn emit_serialize_sleep_missing_property_warning(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        property: &str,
        call_span: Option<php_ir::IrSpan>,
    ) -> Result<(), Box<VmResult>> {
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_VM_SERIALIZE_SLEEP_MISSING_PROPERTY",
            RuntimeSeverity::Warning,
            format!(
                "serialize(): \"{property}\" returned as member variable from __sleep() but does not exist"
            ),
            call_span
                .map(|span| runtime_source_span(compiled, span))
                .unwrap_or_default(),
            stack_trace(compiled, stack),
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LegacySerializablePayload {
    class_name: String,
    payload: PhpString,
}

pub(super) fn legacy_serializable_wire(class_name: &str, payload: &PhpString) -> PhpString {
    let class_bytes = class_name.as_bytes();
    let payload_bytes = payload.as_bytes();
    let mut encoded = Vec::new();
    encoded.extend_from_slice(format!("C:{}:\"", class_bytes.len()).as_bytes());
    encoded.extend_from_slice(class_bytes);
    encoded.extend_from_slice(format!("\":{}:{{", payload_bytes.len()).as_bytes());
    encoded.extend_from_slice(payload_bytes);
    encoded.extend_from_slice(b"}");
    PhpString::from_bytes(encoded)
}

pub(super) fn parse_legacy_serializable_payload(
    input: &PhpString,
) -> Option<LegacySerializablePayload> {
    let bytes = input.as_bytes();
    let mut offset = 0usize;
    expect_serialized_byte(bytes, &mut offset, b'C')?;
    expect_serialized_byte(bytes, &mut offset, b':')?;
    let class_len = take_serialized_usize(bytes, &mut offset, b':')?;
    expect_serialized_byte(bytes, &mut offset, b'"')?;
    let class = take_serialized_bytes(bytes, &mut offset, class_len)?;
    expect_serialized_byte(bytes, &mut offset, b'"')?;
    expect_serialized_byte(bytes, &mut offset, b':')?;
    let payload_len = take_serialized_usize(bytes, &mut offset, b':')?;
    expect_serialized_byte(bytes, &mut offset, b'{')?;
    let payload = take_serialized_bytes(bytes, &mut offset, payload_len)?;
    expect_serialized_byte(bytes, &mut offset, b'}')?;
    if offset != bytes.len() {
        return None;
    }
    Some(LegacySerializablePayload {
        class_name: String::from_utf8_lossy(&class).into_owned(),
        payload: PhpString::from_bytes(payload),
    })
}

pub(super) fn parse_indexed_serialized_object_payload(
    input: &PhpString,
) -> Option<(String, PhpArray)> {
    let bytes = trim_serialized_outer_ascii_whitespace(input.as_bytes());
    let mut offset = 0usize;
    expect_serialized_byte(bytes, &mut offset, b'O')?;
    expect_serialized_byte(bytes, &mut offset, b':')?;
    let class_len = take_serialized_usize(bytes, &mut offset, b':')?;
    expect_serialized_byte(bytes, &mut offset, b'"')?;
    let class = take_serialized_bytes(bytes, &mut offset, class_len)?;
    expect_serialized_byte(bytes, &mut offset, b'"')?;
    expect_serialized_byte(bytes, &mut offset, b':')?;
    let payload_len = take_serialized_usize(bytes, &mut offset, b':')?;
    expect_serialized_byte(bytes, &mut offset, b'{')?;
    if bytes.last().copied() != Some(b'}') {
        return None;
    }
    let payload = &bytes[offset..bytes.len().saturating_sub(1)];
    let mut array_wire = Vec::new();
    array_wire.extend_from_slice(format!("a:{payload_len}:{{").as_bytes());
    array_wire.extend_from_slice(payload);
    array_wire.extend_from_slice(b"}");
    let Value::Array(array) = unserialize_value(
        &PhpString::from_bytes(array_wire),
        UnserializeOptions::default(),
    )
    .ok()?
    else {
        return None;
    };
    Some((String::from_utf8_lossy(&class).into_owned(), array))
}

pub(super) fn trim_serialized_outer_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(start, |index| index + 1);
    &bytes[start..end]
}

pub(super) fn validate_hash_context_unserialize_payload(payload: &PhpArray) -> Option<String> {
    if hash_context_payload_is_internal_property_shape(payload) {
        return None;
    }
    if payload.len() != 5 {
        return Some(hash_context_ill_formed_message());
    }
    let algorithm = match payload.get(&ArrayKey::Int(0)).map(effective_value) {
        Some(Value::String(algorithm)) => algorithm.to_string_lossy(),
        _ => return Some(hash_context_ill_formed_message()),
    };
    let flags = match payload.get(&ArrayKey::Int(1)).map(effective_value) {
        Some(Value::Int(flags)) => flags,
        _ => return Some(hash_context_ill_formed_message()),
    };
    if flags & HASH_HMAC_FLAG != 0 {
        return Some("HashContext with HASH_HMAC option cannot be serialized".to_owned());
    }
    if !php_runtime::api::hash_algorithm_exists(&algorithm) {
        return Some("Unknown hash algorithm".to_owned());
    }
    let internals = match payload.get(&ArrayKey::Int(2)).map(effective_value) {
        Some(Value::Array(internals)) => internals,
        _ => return Some(hash_context_ill_formed_code_message(&algorithm, -1)),
    };
    let magic = match payload.get(&ArrayKey::Int(3)).map(effective_value) {
        Some(Value::Int(magic)) => magic,
        _ => return Some(hash_context_ill_formed_code_message(&algorithm, -1)),
    };
    if !hash_context_serialization_magic_is_supported(magic) {
        return Some(hash_context_ill_formed_code_message(&algorithm, -1));
    }
    if !matches!(
        payload.get(&ArrayKey::Int(4)).map(effective_value),
        Some(Value::Array(_))
    ) {
        return Some(hash_context_ill_formed_code_message(&algorithm, -1));
    }
    validate_hash_context_serialized_internals(&algorithm, &internals)
}

pub(super) fn hash_context_payload_is_internal_property_shape(payload: &PhpArray) -> bool {
    !payload.is_empty()
        && payload.get(&ArrayKey::Int(0)).is_none()
        && payload
            .iter()
            .any(|(key, _)| matches!(key, ArrayKey::String(_)))
}

pub(super) fn validate_hash_context_serialized_internals(
    algorithm: &str,
    internals: &PhpArray,
) -> Option<String> {
    match normalize_hash_algorithm_name(algorithm).as_str() {
        "sha1"
            if !matches!(
                internals.get(&ArrayKey::Int(6)).map(effective_value),
                Some(Value::Int(_))
            ) =>
        {
            return Some(hash_context_ill_formed_code_message(algorithm, -1024));
        }
        "xxh32" if hash_context_serialized_memsize(internals, 10).is_some_and(|size| size > 16) => {
            return Some(hash_context_ill_formed_code_message(algorithm, -2000));
        }
        "xxh64" if hash_context_serialized_memsize(internals, 18).is_some_and(|size| size > 32) => {
            return Some(hash_context_ill_formed_code_message(algorithm, -2000));
        }
        _ => {}
    }
    None
}

pub(super) fn hash_context_serialized_memsize(internals: &PhpArray, index: i64) -> Option<i64> {
    match internals.get(&ArrayKey::Int(index)).map(effective_value) {
        Some(Value::Int(value)) => Some(value),
        _ => None,
    }
}

pub(super) fn hash_context_serialization_magic_is_supported(magic: i64) -> bool {
    matches!(magic, 2 | 100 | 101)
}

pub(super) fn hash_context_ill_formed_message() -> String {
    "Incomplete or ill-formed serialization data".to_owned()
}

pub(super) fn hash_context_ill_formed_code_message(algorithm: &str, code: i64) -> String {
    format!("Incomplete or ill-formed serialization data (\"{algorithm}\" code {code})")
}

pub(super) fn normalize_hash_algorithm_name(algorithm: &str) -> String {
    algorithm.to_ascii_lowercase()
}

pub(super) fn validate_spl_array_container_unserialize_payload(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    payload: &PhpArray,
) -> Option<String> {
    let normalized = normalize_class_name(class_name);
    let required_len = if normalized == "arrayobject" {
        3..=4
    } else {
        3..=3
    };
    if !required_len.contains(&payload.len()) {
        return Some("Incomplete or ill-typed serialization data".to_owned());
    }
    if !matches!(
        payload.get(&ArrayKey::Int(0)).map(effective_value),
        Some(Value::Int(_))
    ) {
        return Some("Incomplete or ill-typed serialization data".to_owned());
    }
    if normalized == "arrayobject"
        && payload.len() == 4
        && !matches!(
            payload.get(&ArrayKey::Int(3)).map(effective_value),
            Some(Value::String(_))
        )
    {
        return Some("Incomplete or ill-typed serialization data".to_owned());
    }
    if !matches!(
        payload.get(&ArrayKey::Int(1)).map(effective_value),
        Some(Value::Array(_) | Value::Object(_))
    ) {
        return Some("Passed variable is not an array or object".to_owned());
    }
    if !matches!(
        payload.get(&ArrayKey::Int(2)).map(effective_value),
        Some(Value::Array(_))
    ) {
        return Some("Incomplete or ill-typed serialization data".to_owned());
    }
    if normalized == "arrayobject" && payload.len() == 4 {
        let Some(Value::String(iterator_class)) =
            payload.get(&ArrayKey::Int(3)).map(effective_value)
        else {
            return Some("Incomplete or ill-typed serialization data".to_owned());
        };
        let iterator_class = iterator_class.to_string_lossy();
        if !class_like_exists_direct(
            compiled,
            state,
            &iterator_class,
            AutoloadClassLookupKind::Class,
        ) {
            return Some(format!(
                "Cannot deserialize ArrayObject with iterator class '{iterator_class}'; no such class exists"
            ));
        }
        if !class_implements_in_state(
            compiled,
            state,
            &iterator_class,
            "Iterator",
            &mut Vec::new(),
        )
        .unwrap_or(false)
        {
            return Some(format!(
                "Cannot deserialize ArrayObject with iterator class '{iterator_class}'; this class does not implement the Iterator interface"
            ));
        }
    }
    None
}

pub(super) fn validate_spl_doubly_linked_list_unserialize_payload(
    payload: &PhpArray,
) -> Option<String> {
    if payload.len() != 3
        || !matches!(
            payload.get(&ArrayKey::Int(0)).map(effective_value),
            Some(Value::Int(_))
        )
        || !matches!(
            payload.get(&ArrayKey::Int(1)).map(effective_value),
            Some(Value::Array(_))
        )
        || !matches!(
            payload.get(&ArrayKey::Int(2)).map(effective_value),
            Some(Value::Array(_))
        )
    {
        return Some("Incomplete or ill-typed serialization data".to_owned());
    }
    None
}

pub(super) fn validate_spl_object_storage_unserialize_payload(
    payload: &PhpArray,
) -> Option<String> {
    if payload.len() != 2 {
        return Some("Incomplete or ill-typed serialization data".to_owned());
    }
    let Some(Value::Array(storage)) = payload.get(&ArrayKey::Int(0)).map(effective_value) else {
        return Some("Incomplete or ill-typed serialization data".to_owned());
    };
    if !matches!(
        payload.get(&ArrayKey::Int(1)).map(effective_value),
        Some(Value::Array(_))
    ) {
        return Some("Incomplete or ill-typed serialization data".to_owned());
    }
    if storage.len() % 2 != 0 {
        return Some("Odd number of elements".to_owned());
    }
    for (index, (_, value)) in storage.iter().enumerate() {
        if index % 2 == 0 && !matches!(effective_value(value), Value::Object(_)) {
            return Some("Non-object key".to_owned());
        }
    }
    None
}

pub(super) fn take_serialized_usize(
    bytes: &[u8],
    offset: &mut usize,
    delimiter: u8,
) -> Option<usize> {
    let start = *offset;
    while *offset < bytes.len() && bytes[*offset] != delimiter {
        *offset += 1;
    }
    if *offset >= bytes.len() {
        return None;
    }
    let value = std::str::from_utf8(&bytes[start..*offset])
        .ok()?
        .parse::<usize>()
        .ok()?;
    *offset += 1;
    Some(value)
}

pub(super) fn take_serialized_bytes(
    bytes: &[u8],
    offset: &mut usize,
    length: usize,
) -> Option<Vec<u8>> {
    let end = offset.checked_add(length)?;
    if end > bytes.len() {
        return None;
    }
    let value = bytes[*offset..end].to_vec();
    *offset = end;
    Some(value)
}

pub(super) fn expect_serialized_byte(bytes: &[u8], offset: &mut usize, expected: u8) -> Option<()> {
    let actual = bytes.get(*offset).copied()?;
    if actual != expected {
        return None;
    }
    *offset += 1;
    Some(())
}
