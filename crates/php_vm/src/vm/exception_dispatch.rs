use super::prelude::*;

impl Vm {
    pub(super) fn throw_exception_result(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &CallStack,
        state: &mut ExecutionState,
        span: php_ir::IrSpan,
        message: String,
    ) -> VmResult {
        let message_value = Value::string(message.into_bytes());
        let throwable = match make_exception_object("Exception", &message_value) {
            Ok(object) => Value::Object(object),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        tag_throwable_location(&throwable, compiled, span);
        state.pending_trace = Some(capture_backtrace_string(compiled, stack));
        state.pending_throw = Some(throwable);
        VmResult::propagating_exception(output.clone())
    }

    pub(super) fn throw_catchable_exception(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &CallStack,
        state: &mut ExecutionState,
        message: String,
    ) -> VmResult {
        let message_value = Value::string(message.into_bytes());
        let throwable = match make_exception_object("Exception", &message_value) {
            Ok(object) => Value::Object(object),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        state.pending_trace = Some(capture_backtrace_string(compiled, stack));
        state.pending_throw = Some(throwable);
        VmResult::propagating_exception(output.clone())
    }

    #[cold]
    #[inline(never)]
    pub(super) fn runtime_error(
        &self,
        output: &OutputBuffer,
        compiled: &CompiledUnit,
        stack: &CallStack,
        message: impl Into<String>,
    ) -> VmResult {
        let mut message = message.into();
        let diagnostic_message = message.clone();
        if stack.len() > 1 {
            message.push_str("\ncall_stack:");
            for frame in stack.frames().iter().rev() {
                let name = compiled
                    .unit()
                    .functions
                    .get(frame.function.index())
                    .map(|function| function.name.as_str())
                    .unwrap_or("<missing>");
                message.push_str("\n  at ");
                message.push_str(name);
            }
        }
        let diagnostic = runtime_diagnostic_for_message(&diagnostic_message, compiled, stack);
        VmResult::runtime_error_with_diagnostic(output.clone(), message, diagnostic)
    }

    pub(super) fn runtime_error_with_source_span(
        &self,
        output: &OutputBuffer,
        compiled: &CompiledUnit,
        stack: &CallStack,
        source_span: RuntimeSourceSpan,
        message: impl Into<String>,
    ) -> VmResult {
        let mut message = message.into();
        let diagnostic_message = message.clone();
        if stack.len() > 1 {
            message.push_str("\ncall_stack:");
            for frame in stack.frames().iter().rev() {
                let name = compiled
                    .unit()
                    .functions
                    .get(frame.function.index())
                    .map(|function| function.name.as_str())
                    .unwrap_or("<missing>");
                message.push_str("\n  at ");
                message.push_str(name);
            }
        }
        let diagnostic = runtime_diagnostic_for_message_with_source_span(
            &diagnostic_message,
            compiled,
            stack,
            source_span,
        );
        VmResult::runtime_error_with_diagnostic(output.clone(), message, diagnostic)
    }

    pub(super) fn runtime_error_with_bringup_context(
        &self,
        view: ExecutionView<'_>,
        source_span: RuntimeSourceSpan,
        message: impl Into<String>,
        context: BringupDiagnosticInput,
    ) -> VmResult {
        let ExecutionView {
            compiled,
            output,
            stack,
            state,
        } = view;
        let mut message = message.into();
        let diagnostic_message = message.clone();
        if stack.len() > 1 {
            message.push_str("\ncall_stack:");
            for frame in stack.frames().iter().rev() {
                let name = compiled
                    .unit()
                    .functions
                    .get(frame.function.index())
                    .map(|function| function.name.as_str())
                    .unwrap_or("<missing>");
                message.push_str("\n  at ");
                message.push_str(name);
            }
        }
        let mut diagnostic = runtime_diagnostic_for_message_with_source_span(
            &diagnostic_message,
            compiled,
            stack,
            source_span,
        );
        if let Some(payload) = runtime_bringup_payload(
            &diagnostic_message,
            diagnostic.id(),
            compiled,
            state,
            stack,
            context,
        ) {
            diagnostic = diagnostic.with_diagnostic_payload(payload);
        }
        VmResult::runtime_error_with_diagnostic(output.clone(), message, diagnostic)
    }

    /// Records `throwable` as unwinding past the current frame, pops that frame,
    /// and returns a non-success result so the caller re-throws it through its
    /// own handlers (or, at the entry point, renders it as uncaught).
    pub(super) fn propagate_exception(
        &self,
        output: &OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        throwable: Value,
    ) -> VmResult {
        state.pending_throw = Some(throwable);
        stack.pop_recycle();
        VmResult::propagating_exception(output.clone())
    }

    /// Builds a runtime error for `message` and routes PHP throwables through
    /// the current frame's handlers.
    pub(super) fn raise_runtime_error(
        &self,
        cursor: ExecutionCursor<'_>,
        handlers: &mut Vec<ExceptionHandler>,
        pending_control: &mut Option<PendingControl>,
        span: php_ir::IrSpan,
        message: String,
    ) -> RaiseOutcome {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let result = self.runtime_error_with_bringup_context(
            ExecutionView::new(compiled, output, stack, state),
            runtime_source_span(compiled, span),
            message,
            BringupDiagnosticInput {
                autoload_enabled: Some(true),
                ..BringupDiagnosticInput::default()
            },
        );
        if let Some(throwable) = runtime_error_throwable(&result) {
            tag_throwable_location(&throwable, compiled, span);
            state.pending_trace = Some(capture_backtrace_string(compiled, stack));
            if let Some(target) = handle_throw(
                compiled,
                throwable.clone(),
                stack,
                state,
                handlers,
                pending_control,
            ) {
                return RaiseOutcome::Caught(target);
            }
            return RaiseOutcome::Done(Box::new(
                self.propagate_exception(output, stack, state, throwable),
            ));
        }
        RaiseOutcome::Done(Box::new(result))
    }

    pub(super) fn raise_runtime_class_entry_error(
        &self,
        cursor: ExecutionCursor<'_>,
        handlers: &mut Vec<ExceptionHandler>,
        pending_control: &mut Option<PendingControl>,
        operation_span: IrSpan,
        error: RuntimeClassEntryError,
    ) -> RaiseOutcome {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let location_span = error.constant_initializer_span.unwrap_or(operation_span);
        let trace = error
            .constant_initializer_span
            .map(|_| {
                capture_backtrace_string_with_constant_expression(compiled, stack, operation_span)
            })
            .unwrap_or_else(|| capture_backtrace_string(compiled, stack));
        let result = self.runtime_error(output, compiled, stack, error.message);
        if let Some(throwable) = runtime_error_throwable(&result) {
            tag_throwable_location(&throwable, compiled, location_span);
            state.pending_trace = Some(trace);
            if let Some(target) = handle_throw(
                compiled,
                throwable.clone(),
                stack,
                state,
                handlers,
                pending_control,
            ) {
                return RaiseOutcome::Caught(target);
            }
            return RaiseOutcome::Done(Box::new(
                self.propagate_exception(output, stack, state, throwable),
            ));
        }
        RaiseOutcome::Done(Box::new(result))
    }

    pub(super) fn route_throwable_result(
        &self,
        cursor: ExecutionCursor<'_>,
        handlers: &mut Vec<ExceptionHandler>,
        pending_control: &mut Option<PendingControl>,
        result: VmResult,
    ) -> RaiseOutcome {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        if vm_result_has_php_fatal_output(&result) {
            return RaiseOutcome::Done(Box::new(result));
        }
        if let Some(throwable) = state
            .pending_throw
            .take()
            .or_else(|| runtime_error_throwable(&result))
        {
            if let Some(target) = handle_throw(
                compiled,
                throwable.clone(),
                stack,
                state,
                handlers,
                pending_control,
            ) {
                return RaiseOutcome::Caught(target);
            }
            return RaiseOutcome::Done(Box::new(
                self.propagate_exception(output, stack, state, throwable),
            ));
        }
        RaiseOutcome::Done(Box::new(result))
    }

    pub(super) fn handle_uncaught_exception(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        value: Value,
    ) -> VmResult {
        // A registered exception handler may itself throw; PHP routes that new
        // exception to the handler active at that point (which the handler may
        // have just re-registered). Loop so a throwing handler is followed by the
        // current handler, capped to avoid runaway recursion.
        let mut value = value;
        for _ in 0..256 {
            let trace = state.pending_trace.take();
            let Some(callback) = state.exception_handlers.last().cloned() else {
                return uncaught_exception(output, compiled, stack, value, trace);
            };
            let result = self.call_callable(
                compiled,
                Value::Callable(Box::new(callback)),
                vec![CallArgument::positional(value)],
                output,
                stack,
                state,
            );
            if result.status.is_success() {
                return VmResult::success_no_output(None);
            }
            match state.pending_throw.take() {
                Some(next) => value = next,
                None => return result,
            }
        }
        let trace = state.pending_trace.take();
        uncaught_exception(output, compiled, stack, value, trace)
    }
}

pub(super) fn vm_result_has_php_fatal_output(result: &VmResult) -> bool {
    result.output.to_string_lossy().contains("Fatal error:")
}

pub(super) fn function_redeclaration_fatal_result(
    output: &mut OutputBuffer,
    compiled: &CompiledUnit,
    stack: &CallStack,
    span: php_ir::IrSpan,
    display_message: String,
) -> VmResult {
    let (file, line) = source_span_file_line(compiled, span).unwrap_or_else(|| {
        let file = compiled
            .unit()
            .files
            .get(span.file.index())
            .map_or_else(String::new, |file| file.path.clone());
        (file, i64::from(span.start))
    });
    output.write_test_str(&format!(
        "Fatal error: {display_message} in {file} on line {line}\n"
    ));
    let message = format!("E_PHP_VM_FUNCTION_REDECLARATION: {display_message}");
    let diagnostic = runtime_diagnostic_for_message_with_source_span(
        &message,
        compiled,
        stack,
        runtime_source_span(compiled, span),
    );
    VmResult::runtime_error_with_diagnostic(output.clone(), message, diagnostic)
}

#[derive(Clone, Debug, Default)]
pub(super) struct BringupDiagnosticInput {
    pub(super) error_class: Option<&'static str>,
    pub(super) requested_name: Option<String>,
    pub(super) lookup_kind: Option<&'static str>,
    pub(super) autoload_enabled: Option<bool>,
    pub(super) builtin_owner: Option<String>,
    pub(super) argument_count: Option<usize>,
    pub(super) argument_types: Option<String>,
}

pub(super) fn runtime_bringup_payload(
    message: &str,
    id: &str,
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    input: BringupDiagnosticInput,
) -> Option<RuntimeDiagnosticPayload> {
    let error_class = input
        .error_class
        .or_else(|| infer_bringup_error_class(id, message))?;
    let lookup_kind = input
        .lookup_kind
        .or_else(|| infer_bringup_lookup_kind(id, message));
    let requested_name = input
        .requested_name
        .or_else(|| lookup_kind.and_then(|kind| requested_name_from_message(message, kind)));
    let normalized_name = requested_name
        .as_deref()
        .zip(lookup_kind)
        .map(|(name, kind)| normalized_bringup_name(name, kind));
    let caller_file = current_source_path(compiled, stack)
        .map(|path| path.to_string_lossy().into_owned())
        .or_else(|| compiled.unit().files.first().map(|file| file.path.clone()));
    let builtin_owner = input.builtin_owner.or_else(|| {
        matches!(error_class, "stdlib_builtin")
            .then(|| {
                requested_name
                    .as_deref()
                    .and_then(infer_builtin_owner_for_name)
            })
            .flatten()
            .map(str::to_owned)
    });

    let mut context = RuntimeBringupDiagnosticContext::new(error_class)
        .with_optional_field("requested_name", requested_name.clone())
        .with_optional_field("normalized_name", normalized_name)
        .with_optional_field("lookup_kind", lookup_kind.map(str::to_owned))
        .with_field(
            "autoload_stack_size",
            state.autoload_registry.callbacks().len().to_string(),
        )
        .with_field(
            "autoload_recursion_depth",
            state.autoload_stack.len().to_string(),
        )
        .with_field(
            "include_path",
            state.ini.get("include_path").unwrap_or(".").to_owned(),
        )
        .with_field("cwd", state.cwd.to_string_lossy())
        .with_optional_field("caller_file", caller_file)
        .with_field("class_table_epoch", state.class_table_epoch.to_string())
        .with_field(
            "autoload_stack_epoch",
            state.autoload_stack_epoch.to_string(),
        );
    if let Some(enabled) = input.autoload_enabled {
        context = context.with_field("autoload_enabled", enabled.to_string());
    }
    if let Some(owner) = builtin_owner {
        context = context.with_field("builtin_owner", owner);
    }
    if let Some(count) = input.argument_count {
        context = context.with_field("argument_count", count.to_string());
    }
    if let Some(types) = input.argument_types {
        context = context.with_field("argument_types", types);
    }
    Some(RuntimeDiagnosticPayload::Bringup(context))
}

pub(super) fn infer_bringup_error_class(id: &str, message: &str) -> Option<&'static str> {
    let lower_message = message.to_ascii_lowercase();
    if id.contains("CALLABLE") || lower_message.contains("callback") {
        return Some("callable_resolution");
    }
    if id.contains("AUTOLOAD")
        || id.contains("UNKNOWN_CLASS")
        || id.contains("UNKNOWN_PARENT_CLASS")
        || id.contains("REFLECTION_UNKNOWN_CLASS")
    {
        return Some("autoload_lookup");
    }
    if id.contains("BUILTIN")
        || id.contains("FUNCTION")
        || id.contains("CONSTANT")
        || id.contains("EXTENSION")
    {
        return Some("stdlib_builtin");
    }
    None
}

pub(super) fn infer_bringup_lookup_kind(id: &str, message: &str) -> Option<&'static str> {
    let lower_message = message.to_ascii_lowercase();
    if id.contains("EXTENSION") || lower_message.contains("extension ") {
        Some("extension")
    } else if id.contains("CONSTANT") || lower_message.contains("constant ") {
        Some("constant")
    } else if id.contains("FUNCTION") || lower_message.contains("function ") {
        Some("function")
    } else if lower_message.contains("interface ") {
        Some("interface")
    } else if lower_message.contains("trait ") {
        Some("trait")
    } else if lower_message.contains("enum ") {
        Some("enum")
    } else if id.contains("CLASS") || lower_message.contains("class ") {
        Some("class")
    } else if id.contains("BUILTIN") {
        Some("function")
    } else {
        None
    }
}

pub(super) fn requested_name_from_message(message: &str, lookup_kind: &str) -> Option<String> {
    if let Some(quoted) = first_double_quoted(message) {
        return Some(quoted);
    }
    match lookup_kind {
        "function" => token_after_any(message, &["function ", "builtin "])
            .map(trim_symbol_token)
            .filter(|name| !name.is_empty()),
        "class" => token_after_any(message, &["class ", "Class "])
            .map(trim_symbol_token)
            .filter(|name| !name.is_empty()),
        "interface" => token_after_any(message, &["interface ", "Interface "])
            .map(trim_symbol_token)
            .filter(|name| !name.is_empty()),
        "trait" => token_after_any(message, &["trait ", "Trait "])
            .map(trim_symbol_token)
            .filter(|name| !name.is_empty()),
        "enum" => token_after_any(message, &["enum ", "Enum "])
            .map(trim_symbol_token)
            .filter(|name| !name.is_empty()),
        "constant" => token_after_any(message, &["constant ", "Constant "])
            .map(trim_symbol_token)
            .filter(|name| !name.is_empty()),
        "extension" => token_after_any(message, &["extension "])
            .map(trim_symbol_token)
            .filter(|name| !name.is_empty()),
        _ => None,
    }
}

pub(super) fn first_double_quoted(message: &str) -> Option<String> {
    let start = message.find('"')?;
    let rest = &message[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

pub(super) fn token_after_any(message: &str, prefixes: &[&str]) -> Option<String> {
    prefixes
        .iter()
        .find_map(|prefix| message.split_once(prefix).map(|(_, rest)| rest.to_owned()))
}

pub(super) fn trim_symbol_token(token: String) -> String {
    token
        .split(|ch: char| {
            ch.is_whitespace() || matches!(ch, '(' | ')' | ':' | ',' | '\'' | '"' | '|')
        })
        .next()
        .unwrap_or_default()
        .trim_matches('\\')
        .to_owned()
}

pub(super) fn normalized_bringup_name(name: &str, lookup_kind: &str) -> String {
    match lookup_kind {
        "class" | "interface" | "trait" | "enum" => normalize_class_name(name),
        "function" => normalize_function_name(name),
        "extension" => name.to_ascii_lowercase(),
        _ => name.to_owned(),
    }
}

pub(super) fn infer_builtin_owner_for_name(name: &str) -> Option<&'static str> {
    php_std::ExtensionRegistry::standard_library()
        .enabled_php_function(name)
        .map(|entry| entry.extension())
        .or_else(|| {
            php_std::ExtensionRegistry::standard_library()
                .enabled_constant(name)
                .map(|entry| entry.extension())
        })
        .or_else(|| {
            php_std::ExtensionRegistry::standard_library()
                .enabled_class(name)
                .map(|entry| entry.extension())
        })
}

pub(super) fn runtime_bringup_payload_without_state(
    message: &str,
    id: &str,
) -> Option<RuntimeDiagnosticPayload> {
    let error_class = infer_bringup_error_class(id, message)?;
    let lookup_kind = infer_bringup_lookup_kind(id, message);
    let requested_name = lookup_kind.and_then(|kind| requested_name_from_message(message, kind));
    let normalized_name = requested_name
        .as_deref()
        .zip(lookup_kind)
        .map(|(name, kind)| normalized_bringup_name(name, kind));
    let builtin_owner = matches!(error_class, "stdlib_builtin")
        .then(|| {
            requested_name
                .as_deref()
                .and_then(infer_builtin_owner_for_name)
        })
        .flatten()
        .map(str::to_owned);
    let context = RuntimeBringupDiagnosticContext::new(error_class)
        .with_optional_field("requested_name", requested_name)
        .with_optional_field("normalized_name", normalized_name)
        .with_optional_field("lookup_kind", lookup_kind.map(str::to_owned))
        .with_optional_field("builtin_owner", builtin_owner);
    Some(RuntimeDiagnosticPayload::Bringup(context))
}

pub(super) fn internal_throwable_display_name(class_name: &str) -> String {
    match normalize_class_name(class_name).as_str() {
        "throwable" => "Throwable".to_owned(),
        "exception" => "Exception".to_owned(),
        "error" => "Error".to_owned(),
        "parseerror" => "ParseError".to_owned(),
        "typeerror" => "TypeError".to_owned(),
        "valueerror" => "ValueError".to_owned(),
        "unhandledmatcherror" => "UnhandledMatchError".to_owned(),
        "argumentcounterror" => "ArgumentCountError".to_owned(),
        "fibererror" => "FiberError".to_owned(),
        "jsonexception" => "JsonException".to_owned(),
        "pdoexception" => "PDOException".to_owned(),
        "reflectionexception" => "ReflectionException".to_owned(),
        "logicexception" => "LogicException".to_owned(),
        "badfunctioncallexception" => "BadFunctionCallException".to_owned(),
        "badmethodcallexception" => "BadMethodCallException".to_owned(),
        "domainexception" => "DomainException".to_owned(),
        "invalidargumentexception" => "InvalidArgumentException".to_owned(),
        "lengthexception" => "LengthException".to_owned(),
        "outofrangeexception" => "OutOfRangeException".to_owned(),
        "runtimeexception" => "RuntimeException".to_owned(),
        "outofboundsexception" => "OutOfBoundsException".to_owned(),
        "overflowexception" => "OverflowException".to_owned(),
        "rangeexception" => "RangeException".to_owned(),
        "underflowexception" => "UnderflowException".to_owned(),
        "unexpectedvalueexception" => "UnexpectedValueException".to_owned(),
        _ => class_name.to_owned(),
    }
}

pub(super) fn internal_throwable_parent(class_name: &str) -> Option<&'static str> {
    match normalize_class_name(class_name).as_str() {
        "parseerror" | "typeerror" | "valueerror" | "unhandledmatcherror" | "fibererror" => {
            Some("Error")
        }
        "argumentcounterror" => Some("TypeError"),
        "jsonexception"
        | "sodiumexception"
        | "pdoexception"
        | "reflectionexception"
        | "logicexception"
        | "runtimeexception" => Some("Exception"),
        "badfunctioncallexception"
        | "domainexception"
        | "invalidargumentexception"
        | "lengthexception"
        | "outofrangeexception" => Some("LogicException"),
        "badmethodcallexception" => Some("BadFunctionCallException"),
        "outofboundsexception"
        | "overflowexception"
        | "rangeexception"
        | "underflowexception"
        | "unexpectedvalueexception" => Some("RuntimeException"),
        _ => None,
    }
}

const INTERNAL_THROWABLE_CLASS_NAMES: &[&str] = &[
    "exception",
    "error",
    "parseerror",
    "typeerror",
    "valueerror",
    "unhandledmatcherror",
    "argumentcounterror",
    "fibererror",
    "jsonexception",
    "sodiumexception",
    "pdoexception",
    "reflectionexception",
    "logicexception",
    "badfunctioncallexception",
    "badmethodcallexception",
    "domainexception",
    "invalidargumentexception",
    "lengthexception",
    "outofrangeexception",
    "runtimeexception",
    "outofboundsexception",
    "overflowexception",
    "rangeexception",
    "underflowexception",
    "unexpectedvalueexception",
];

pub(super) fn internal_throwable_instanceof(
    object_class: &str,
    target_class: &str,
) -> Option<bool> {
    // Fast in-place reject: almost every receiver is not an engine
    // throwable, and this probe runs on hot property/method paths.
    let trimmed = object_class.trim_start_matches('\\');
    if !INTERNAL_THROWABLE_CLASS_NAMES
        .iter()
        .any(|candidate| trimmed.eq_ignore_ascii_case(candidate))
    {
        return None;
    }
    let mut object_class = normalize_class_name(object_class);
    let target_class = normalize_class_name(target_class);
    if target_class == "throwable" {
        return Some(true);
    }
    loop {
        if object_class == target_class {
            return Some(true);
        }
        let Some(parent) = internal_throwable_parent(&object_class) else {
            return Some(false);
        };
        object_class = normalize_class_name(parent);
    }
}

pub(super) fn is_instantiable_internal_throwable(class_name: &str) -> bool {
    normalize_class_name(class_name) != "throwable"
        && internal_throwable_instanceof(class_name, "throwable").is_some()
}

pub(super) fn throwable_class_name(value: &Value) -> String {
    match value {
        Value::Object(object) => internal_throwable_display_name(&object.class_name()),
        other => value_type_name(other).to_owned(),
    }
}

const THROWABLE_EXCEPTION_STRING_PROPERTY: &str = "private:Exception:string";
const THROWABLE_EXCEPTION_TRACE_PROPERTY: &str = "private:Exception:trace";
const THROWABLE_EXCEPTION_PREVIOUS_PROPERTY: &str = "private:Exception:previous";
const THROWABLE_TRACE_STRING_PROPERTY: &str = "__phrust_trace_string";

pub(super) fn throwable_private_owner(class_name: &str) -> &'static str {
    if internal_throwable_instanceof(class_name, "Error") == Some(true) {
        "Error"
    } else {
        "Exception"
    }
}

pub(super) fn throwable_private_property_name(class_name: &str, name: &str) -> String {
    format!("private:{}:{name}", throwable_private_owner(class_name))
}

pub(super) fn make_exception_object(
    class_name: &str,
    message: &Value,
) -> Result<ObjectRef, String> {
    let message = to_string(message)?.to_string_lossy();
    let class_name = internal_throwable_display_name(class_name);
    let parent = internal_throwable_parent(&class_name).map(str::to_owned);
    let protected_flags = RuntimeClassPropertyFlags {
        is_protected: true,
        ..RuntimeClassPropertyFlags::default()
    };
    let private_flags = RuntimeClassPropertyFlags {
        is_private: true,
        ..RuntimeClassPropertyFlags::default()
    };
    let throwable_property =
        |name: &str,
         default: Value,
         type_: Option<RuntimeType>,
         flags: RuntimeClassPropertyFlags| RuntimeClassPropertyEntry {
            name: name.to_owned(),
            default,
            type_,
            flags,
            hooks: RuntimeClassPropertyHooks::default(),
            attributes: Vec::new(),
        };
    let class = RuntimeClassEntry {
        name: normalize_class_name(&class_name).into(),
        parent,
        interfaces: vec!["throwable".to_owned()],
        methods: Vec::new(),
        properties: vec![
            throwable_property(
                "message",
                Value::String(PhpString::from_test_str(&message)),
                Some(RuntimeType::String),
                protected_flags,
            ),
            throwable_property(
                &throwable_private_property_name(&class_name, "string"),
                Value::string(Vec::new()),
                Some(RuntimeType::String),
                private_flags,
            ),
            throwable_property(
                "code",
                Value::Int(0),
                Some(RuntimeType::Int),
                protected_flags,
            ),
            throwable_property(
                "file",
                Value::string(Vec::new()),
                Some(RuntimeType::String),
                protected_flags,
            ),
            throwable_property(
                "line",
                Value::Int(0),
                Some(RuntimeType::Int),
                protected_flags,
            ),
            throwable_property(
                &throwable_private_property_name(&class_name, "trace"),
                Value::Array(PhpArray::new()),
                None,
                private_flags,
            ),
            throwable_property(
                &throwable_private_property_name(&class_name, "previous"),
                Value::Null,
                None,
                private_flags,
            ),
        ],
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: RuntimeClassFlags::default(),
    };
    Ok(ObjectRef::new_with_display_name(&class, class_name))
}

pub(super) fn throwable_property_storage_name(name: &str) -> &str {
    match name {
        "string" => THROWABLE_EXCEPTION_STRING_PROPERTY,
        "trace" => THROWABLE_EXCEPTION_TRACE_PROPERTY,
        "previous" => THROWABLE_EXCEPTION_PREVIOUS_PROPERTY,
        "trace_string" => THROWABLE_TRACE_STRING_PROPERTY,
        _ => name,
    }
}

pub(super) fn throwable_property_storage_name_for_object(object: &ObjectRef, name: &str) -> String {
    match name {
        "string" | "trace" | "previous" => {
            throwable_private_property_name(&object.class_name(), name)
        }
        "trace_string" => THROWABLE_TRACE_STRING_PROPERTY.to_owned(),
        _ => name.to_owned(),
    }
}

pub(super) fn get_throwable_property(object: &ObjectRef, name: &str) -> Option<Value> {
    let storage_name = throwable_property_storage_name_for_object(object, name);
    object
        .get_property(&storage_name)
        .or_else(|| object.get_property(throwable_property_storage_name(name)))
        .or_else(|| {
            (storage_name != name)
                .then(|| object.get_property(name))
                .flatten()
        })
}

pub(super) fn set_throwable_property(object: &ObjectRef, name: &str, value: Value) {
    object.set_property(
        throwable_property_storage_name_for_object(object, name),
        value,
    );
}

pub(super) fn new_internal_throwable_object(
    compiled: &CompiledUnit,
    stack: &CallStack,
    class_name: &str,
    args: &[CallArgument],
    span: php_ir::IrSpan,
) -> Result<ObjectRef, String> {
    if args.len() > 3 {
        return Err(format!(
            "E_PHP_VM_TOO_MANY_ARGS: {}::__construct() expects at most 3 arguments, {} given",
            internal_throwable_display_name(class_name),
            args.len()
        ));
    }
    let message = args
        .first()
        .map(|arg| arg.value.clone())
        .unwrap_or_else(|| Value::string(Vec::new()));
    let object = make_exception_object(class_name, &message)?;
    initialize_internal_throwable_object(compiled, stack, &object, class_name, args, span)?;
    Ok(object)
}

pub(super) fn initialize_internal_throwable_object(
    compiled: &CompiledUnit,
    stack: &CallStack,
    object: &ObjectRef,
    class_name: &str,
    args: &[CallArgument],
    span: php_ir::IrSpan,
) -> Result<(), String> {
    if args.len() > 3 {
        return Err(format!(
            "E_PHP_VM_TOO_MANY_ARGS: {}::__construct() expects at most 3 arguments, {} given",
            internal_throwable_display_name(class_name),
            args.len()
        ));
    }
    let message = args
        .first()
        .map(|arg| arg.value.clone())
        .unwrap_or_else(|| Value::string(Vec::new()));
    set_throwable_property(object, "message", message);
    set_throwable_property(object, "code", Value::Int(0));
    if let Some(code) = args.get(1) {
        set_throwable_property(object, "code", code.value.clone());
    }
    if let Some(previous) = args.get(2) {
        set_throwable_property(object, "previous", previous.value.clone());
    }
    let display_span = runtime_source_span(compiled, span);
    if let Some(file) = display_span.file {
        set_throwable_property(object, "file", Value::string(file.into_bytes()));
    }
    if let Some(line) = source_span_display_line(compiled, span, false) {
        set_throwable_property(object, "line", Value::Int(line));
    }
    set_throwable_property(
        object,
        "trace",
        Value::Array(debug_backtrace_array(compiled, stack, 1, 0)),
    );
    set_throwable_property(
        object,
        "trace_string",
        Value::string(capture_backtrace_string(compiled, stack).into_bytes()),
    );
    Ok(())
}

pub(super) fn std_class_entry() -> RuntimeClassEntry {
    RuntimeClassEntry {
        name: normalize_class_name("stdClass").into(),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: RuntimeClassFlags::default(),
    }
}

pub(super) fn incomplete_class_object(class_name: String, source: ObjectRef) -> ObjectRef {
    let class = RuntimeClassEntry {
        name: normalize_class_name("__PHP_Incomplete_Class").into(),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: RuntimeClassFlags::default(),
    };
    let object = ObjectRef::new_with_display_name(&class, "__PHP_Incomplete_Class");
    object.set_property(
        "__PHP_Incomplete_Class_Name",
        Value::string(class_name.into_bytes()),
    );
    for (property, value) in source.properties_snapshot() {
        object.set_property(property, value);
    }
    object
}

pub(super) fn empty_runtime_class(name: &str) -> RuntimeClassEntry {
    RuntimeClassEntry {
        name: normalize_class_name(name).into(),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: RuntimeClassFlags::default(),
    }
}

pub(super) fn internal_throwable_method_value(
    object: &ObjectRef,
    method: &str,
    args: Vec<Value>,
) -> Result<Value, String> {
    let normalized = normalize_method_name(method);
    if !args.is_empty()
        && matches!(
            normalized.as_str(),
            "getmessage"
                | "getcode"
                | "getline"
                | "getfile"
                | "getprevious"
                | "gettraceasstring"
                | "gettrace"
                | "__tostring"
        )
    {
        return Err(format!(
            "E_PHP_VM_TOO_MANY_ARGS: {}::{method}() expects exactly 0 arguments, {} given",
            object.class_name(),
            args.len()
        ));
    }
    match normalized.as_str() {
        "getmessage" => Ok(object
            .get_property("message")
            .unwrap_or_else(|| Value::string(Vec::new()))),
        "getcode" => Ok(get_throwable_property(object, "code").unwrap_or(Value::Int(0))),
        "getline" => Ok(get_throwable_property(object, "line").unwrap_or(Value::Int(0))),
        "getfile" => Ok(object
            .get_property("file")
            .unwrap_or_else(|| Value::string(Vec::new()))),
        "getprevious" => Ok(get_throwable_property(object, "previous").unwrap_or(Value::Null)),
        "gettraceasstring" => Ok(Value::string(throwable_trace_string(object).into_bytes())),
        "gettrace" => Ok(object
            .get_property(throwable_property_storage_name("trace"))
            .unwrap_or_else(|| Value::Array(PhpArray::new()))),
        "__tostring" => Ok(Value::string(throwable_string(object).into_bytes())),
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {}::{method} is not declared",
            object.class_name()
        )),
    }
}

pub(super) fn throwable_trace_string(object: &ObjectRef) -> String {
    get_throwable_property(object, "trace_string")
        .and_then(|value| to_string(&value).ok())
        .map(|value| value.to_string_lossy())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "#0 {main}".to_owned())
}

pub(super) fn throwable_string(object: &ObjectRef) -> String {
    let class_name = internal_throwable_display_name(&object.class_name());
    let message = object
        .get_property("message")
        .and_then(|value| to_string(&value).ok())
        .map(|value| value.to_string_lossy())
        .unwrap_or_default();
    let file = object
        .get_property("file")
        .and_then(|value| to_string(&value).ok())
        .map(|value| value.to_string_lossy())
        .unwrap_or_default();
    let line = match get_throwable_property(object, "line") {
        Some(Value::Int(line)) => line,
        _ => 0,
    };
    let heading = if message.is_empty() {
        format!("{class_name} in {file}:{line}")
    } else {
        format!("{class_name}: {message} in {file}:{line}")
    };
    format!(
        "{heading}\nStack trace:\n{}",
        throwable_trace_string(object)
    )
}

pub(super) fn handle_throw(
    compiled: &CompiledUnit,
    value: Value,
    stack: &mut CallStack,
    state: &ExecutionState,
    handlers: &mut Vec<ExceptionHandler>,
    pending_control: &mut Option<PendingControl>,
) -> Option<BlockId> {
    while let Some(handler) = handlers.pop() {
        if let Some(catch) = handler.catch
            && catch_matches(compiled, state, &value, &handler.catch_types).unwrap_or(false)
        {
            if let Some(local) = handler.exception_local {
                let previous = stack
                    .current()
                    .and_then(|frame| frame.locals.get(local))
                    .unwrap_or(Value::Uninitialized);
                if let Some(frame) = stack.current_mut() {
                    let _ = frame.locals.set(local, value.clone());
                }
                release_unrooted_object_handles(&previous);
            }
            return Some(catch);
        }
        if let Some(finally) = handler.finally {
            *pending_control = Some(PendingControl::Throw(value));
            return Some(finally);
        }
    }
    None
}

pub(super) fn catch_matches(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    value: &Value,
    catch_types: &[String],
) -> Result<bool, String> {
    if catch_types.is_empty() {
        return Ok(true);
    }
    for catch_type in catch_types {
        if object_instanceof_in_state(compiled, state, value, catch_type)? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn uncaught_exception(
    output: &mut OutputBuffer,
    compiled: &CompiledUnit,
    stack: &CallStack,
    value: Value,
    trace: Option<String>,
) -> VmResult {
    let class_name = throwable_class_name(&value);
    let (message, file, line) = match &value {
        Value::Object(object) => {
            let message = object
                .get_property("message")
                .and_then(|value| to_string(&value).ok())
                .map(|value| value.to_string_lossy())
                .unwrap_or_default();
            let file = object
                .get_property("file")
                .and_then(|value| to_string(&value).ok())
                .map(|value| value.to_string_lossy())
                .unwrap_or_default();
            let line = match get_throwable_property(object, "line") {
                Some(Value::Int(line)) => line,
                _ => 0,
            };
            (message, file, line)
        }
        other => (
            format!("uncaught {}", value_type_name(other)),
            String::new(),
            0,
        ),
    };
    let has_definition_location =
        class_name == "TypeError" && message.contains("called in ") && !file.is_empty() && line > 0;
    let has_embedded_location = has_definition_location;
    let heading = if message.is_empty() {
        format!("Uncaught {class_name}")
    } else if has_definition_location {
        format!("Uncaught {class_name}: {message} and defined in {file}:{line}")
    } else {
        format!("Uncaught {class_name}: {message}")
    };
    // PHP renders an uncaught throwable as a fatal error on the output stream.
    let trace = trace.unwrap_or_else(|| "#0 {main}".to_owned());
    let diagnostic_stack =
        stack_trace_from_captured_trace(&trace).unwrap_or_else(|| stack_trace(compiled, stack));
    if has_embedded_location {
        output.write_test_str(&format!(
            "\nFatal error: {heading}\nStack trace:\n{trace}\n  thrown in {file} on line {line}\n"
        ));
    } else {
        output.write_test_str(&format!(
            "\nFatal error: {heading} in {file}:{line}\nStack trace:\n{trace}\n  thrown in {file} on line {line}\n"
        ));
    }
    let full = format!("E_PHP_VM_UNCAUGHT_EXCEPTION: {heading}");
    let mut diagnostic = RuntimeDiagnostic::new(
        "E_PHP_VM_UNCAUGHT_EXCEPTION",
        RuntimeSeverity::FatalError,
        full.clone(),
        RuntimeSourceSpan::default(),
        diagnostic_stack,
        None,
    );
    if let Some(payload) = runtime_bringup_payload_without_state(&full, diagnostic.id()) {
        diagnostic = diagnostic.with_diagnostic_payload(payload);
    }
    VmResult::runtime_error_with_diagnostic(output.clone(), full.clone(), diagnostic)
}

pub(super) fn stack_trace_from_captured_trace(trace: &str) -> Option<Vec<RuntimeStackFrame>> {
    let frames = trace
        .lines()
        .filter_map(runtime_stack_frame_from_trace_line)
        .collect::<Vec<_>>();
    (!frames.is_empty()).then_some(frames)
}

pub(super) fn runtime_stack_frame_from_trace_line(line: &str) -> Option<RuntimeStackFrame> {
    let (_, rest) = line.split_once(' ')?;
    if rest == "{main}" {
        return Some(RuntimeStackFrame::new("main"));
    }
    let (_, call) = rest.split_once("): ")?;
    let function = call.split_once('(').map_or(call, |(name, _)| name);
    let function = function.trim();
    if function.is_empty() {
        None
    } else {
        Some(RuntimeStackFrame::new(function))
    }
}
