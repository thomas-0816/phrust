use super::prelude::*;

impl Vm {
    pub(super) fn execute_stream_wrapper_register(
        &self,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> VmResult {
        if values.len() < 2 || values.len() > 3 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_BUILTIN_ARITY: stream_wrapper_register expects two or three argument(s), {} given",
                    values.len()
                ),
            );
        }
        let protocol = match to_string(&effective_value(&values[0])) {
            Ok(protocol) => protocol.to_string_lossy(),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let class_name = match to_string(&effective_value(&values[1])) {
            Ok(class_name) => class_name.to_string_lossy(),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let Some(class) = lookup_class_in_state(compiled, state, &class_name) else {
            return VmResult::success_no_output(Some(Value::Bool(false)));
        };
        let registered = state.builtins.user_stream_wrappers.register(
            &protocol,
            &class.name,
            &class.display_name,
        );
        VmResult::success_no_output(Some(Value::Bool(registered)))
    }

    pub(super) fn execute_stream_get_wrappers_with_user_wrappers(
        &self,
        values: Vec<Value>,
        state: &ExecutionState,
    ) -> VmResult {
        if !values.is_empty() {
            return VmResult::success_no_output(Some(Value::Bool(false)));
        }
        let mut wrappers = vec![Value::string("file"), Value::string("php")];
        wrappers.extend(
            state
                .builtins
                .user_stream_wrappers
                .protocols()
                .into_iter()
                .map(Value::string),
        );
        VmResult::success_no_output(Some(Value::packed_array(wrappers)))
    }

    pub(super) fn try_execute_user_stream_fopen(
        &self,
        values: Vec<Value>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> Option<VmResult> {
        let uri = match values.first().map(effective_value) {
            Some(value) => match to_string(&value) {
                Ok(uri) => uri.to_string_lossy(),
                Err(message) => {
                    return Some(self.runtime_error(output, compiled, stack, message));
                }
            },
            None => return None,
        };
        let wrapper = state.builtins.user_stream_wrappers.wrapper_for_uri(&uri)?;
        let mode = match values.get(1).map(effective_value) {
            Some(value) => match to_string(&value) {
                Ok(mode) => mode.to_string_lossy(),
                Err(message) => {
                    return Some(self.runtime_error(output, compiled, stack, message));
                }
            },
            None => "r".to_owned(),
        };
        let Some(class) = lookup_class_in_state(compiled, state, &wrapper.class_name) else {
            return Some(VmResult::success_no_output(Some(Value::Bool(false))));
        };
        if lookup_resolved_method_in_state(compiled, state, &class.name, "stream_open", None)
            .ok()
            .flatten()
            .is_none()
        {
            return Some(VmResult::success_no_output(Some(Value::Bool(false))));
        }
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
            Err(error) => {
                return Some(self.runtime_error(output, compiled, stack, error.into_message()));
            }
        };
        if let Err(message) = validate_object_mvp(&runtime_class) {
            return Some(self.runtime_error(output, compiled, stack, message));
        }
        let object = ObjectRef::new_with_display_name(&runtime_class, wrapper.display_class_name);
        let opened_path = ReferenceCell::new(Value::Null);
        let open_result = self.call_object_method_callable(
            ExecutionCursor::new(compiled, output, stack, state),
            object.clone(),
            "stream_open",
            vec![
                CallArgument::positional(Value::string(uri.clone())),
                CallArgument::positional(Value::string(mode.clone())),
                CallArgument::positional(Value::Int(0)),
                CallArgument::positional(Value::Reference(opened_path)),
            ],
            call_span,
        );
        if let Some(throwable) = state.pending_throw.take() {
            return Some(self.handle_uncaught_exception(compiled, output, stack, state, throwable));
        }
        if !open_result.status.is_success() {
            return Some(open_result);
        }
        let opened = open_result
            .return_value
            .as_ref()
            .map_or(Ok(false), to_bool)
            .unwrap_or(false);
        if !opened {
            return Some(VmResult::success_no_output(Some(Value::Bool(false))));
        }
        let resource = state.resources.register_stream(
            php_runtime::api::StreamFlags::new(true, true, true),
            php_runtime::api::StreamMetadata::new(&wrapper.protocol, "stream", &mode, &uri),
        );
        state
            .builtins
            .user_stream_wrappers
            .register_open_stream(&resource, object);
        Some(VmResult::success_no_output(Some(Value::Resource(resource))))
    }

    pub(super) fn try_execute_user_stream_fclose(
        &self,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> Option<VmResult> {
        let Some(Value::Resource(resource)) = values.first().map(effective_value) else {
            return None;
        };
        self.close_user_stream_resource(resource.id(), call_span, output, stack, state, compiled)
    }

    pub(super) fn close_user_stream_resource(
        &self,
        id: php_runtime::api::ResourceId,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> Option<VmResult> {
        let object = state
            .builtins
            .user_stream_wrappers
            .pending_close_object(id)?;
        if lookup_resolved_method_in_state(
            compiled,
            state,
            &object.class_name(),
            "stream_close",
            None,
        )
        .ok()
        .flatten()
        .is_some()
        {
            let close_result = self.call_object_method_callable(
                ExecutionCursor::new(compiled, output, stack, state),
                object,
                "stream_close",
                Vec::new(),
                call_span,
            );
            if let Some(throwable) = state.pending_throw.take() {
                return Some(
                    self.handle_uncaught_exception(compiled, output, stack, state, throwable),
                );
            }
            if !close_result.status.is_success() {
                return Some(close_result);
            }
        }
        Some(VmResult::success_no_output(Some(Value::Bool(
            state.resources.close(id),
        ))))
    }

    pub(super) fn run_shutdown_user_stream_wrappers(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        state: &mut ExecutionState,
    ) -> Result<Vec<RuntimeDiagnostic>, Box<VmResult>> {
        let mut diagnostics = Vec::new();
        for id in state.builtins.user_stream_wrappers.pending_close_ids() {
            let mut stack = CallStack::new();
            let Some(result) =
                self.close_user_stream_resource(id, None, output, &mut stack, state, compiled)
            else {
                continue;
            };
            if !result.status.is_success() {
                return Err(Box::new(result));
            }
            diagnostics.extend(result.diagnostics);
        }
        Ok(diagnostics)
    }
}
