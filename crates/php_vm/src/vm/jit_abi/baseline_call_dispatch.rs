//! Baseline-native dynamic call and prepared-builtin continuation.
//!
//! Rust `Value` materialization and cold-context recovery are confined here.
//! Optimizing artifacts cannot import these ABIs except as their one explicit
//! baseline continuation.

use super::*;
use php_runtime::api::PhpString;
use php_runtime::api::Value;

fn execute_native_enum_static_method(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::CallStaticMethod {
        class_name, method, ..
    } = &instruction.kind
    else {
        return None;
    };
    let class =
        native_active_class_handle(context, class_name).filter(|class| class.flags.is_enum)?;
    if method.eq_ignore_ascii_case("cases") {
        let mut entries: Vec<php_jit::JitNativeDirectArrayEntry> =
            Vec::with_capacity(class.enum_cases.len());
        for (index, case) in class.enum_cases.iter().enumerate() {
            let value = match encode_native_enum_case(context, &class, case) {
                Ok(value) => value,
                Err(error) => {
                    for entry in entries {
                        let _ = context.release(entry.value);
                    }
                    return Some(Err(error));
                }
            };
            entries.push(php_jit::JitNativeDirectArrayEntry {
                key: i64::try_from(index).unwrap_or(i64::MAX),
                value,
            });
        }
        return Some(context.publish_owned_direct_array_entries(entries));
    }
    if method.eq_ignore_ascii_case("from") || method.eq_ignore_ascii_case("tryFrom") {
        let Some(argument) = arguments.first() else {
            return Some(Err(format!(
                "{class_name}::{method}() expects exactly 1 argument"
            )));
        };
        for case in &class.enum_cases {
            let Some(constant) = case
                .value
                .and_then(|value| context.unit.constants.get(value.index()))
                .cloned()
            else {
                continue;
            };
            let candidate = match context.encode_native_ir_constant_owned(&constant) {
                Ok(candidate) => candidate,
                Err(error) => return Some(Err(error)),
            };
            let matches =
                context.native_encoded_values_identical(*argument, candidate) == Some(true);
            if let Err(error) = context.release(candidate) {
                return Some(Err(error));
            }
            if matches {
                return Some(encode_native_enum_case(context, &class, case));
            }
        }
        if method.eq_ignore_ascii_case("tryFrom") {
            return Some(Ok(php_jit::jit_encode_constant(u32::MAX)));
        }
        return Some(Err(format!(
            "E_PHP_THROW:ValueError:{} is not a valid backing value for enum {}",
            context.native_encoded_type_name(*argument),
            class.display_name
        )));
    }
    None
}

fn missing_native_class_dependency(
    context: &mut NativeRequestColdState<'_>,
    display_name: &str,
    span: php_ir::IrSpan,
) -> NativeCallControl {
    let message = format!("Class \"{display_name}\" not found");
    match encode_native_throwable_at(context, "Error", &message, span) {
        Ok(value) => NativeCallControl::Propagate {
            status: php_jit::JitCallStatus::THROW,
            value,
        },
        Err(error) => NativeCallControl::RuntimeError(format!(
            "native missing-class throwable could not be encoded: {error}"
        )),
    }
}

fn ensure_native_class_parent_is_available(
    context: &mut NativeRequestColdState<'_>,
    class: &crate::compiled_unit::CompiledClass,
    source: &php_ir::Instruction,
) -> Result<(), NativeCallControl> {
    let mut current = class.clone();
    let mut visited = std::collections::BTreeSet::new();
    loop {
        if !visited.insert(current.name.clone()) {
            return Err(NativeCallControl::throw(
                "Error",
                format!(
                    "native class hierarchy for {} contains a cycle",
                    class.display_name
                ),
            ));
        }
        let Some(parent) = current.parent.clone() else {
            return Ok(());
        };
        let display_parent = current
            .parent_display_name
            .clone()
            .unwrap_or_else(|| parent.clone());
        let normalized_parent = normalize_class_name(&parent);
        if native_internal_class_is_available(&normalized_parent) {
            return Ok(());
        }
        native_autoload_class(context, &display_parent, source).map_err(NativeCallControl::from)?;
        if let Some(parent) =
            native_active_class_handle(context, &normalized_parent).or_else(|| {
                native_external_class_handle(context, &normalized_parent).map(|(_, class)| class)
            })
        {
            current = parent;
            continue;
        }
        return Err(missing_native_class_dependency(
            context,
            &display_parent,
            current.span,
        ));
    }
}

fn emit_non_reference_return_binding_notice(
    context: &mut NativeRequestColdState<'_>,
    source: &php_ir::Instruction,
    returns_by_reference: bool,
) {
    if returns_by_reference
        || !matches!(
            source.kind,
            php_ir::InstructionKind::BindReferenceFromCall { .. }
        )
    {
        return;
    }
    let path = context
        .unit
        .files
        .get(source.span.file.index())
        .map_or("<unknown>", |file| file.path.as_str());
    let line = native_source_line(context, source);
    context.output.write_bytes(format!(
        "\nNotice: Only variables should be assigned by reference in {path} on line {line}\n"
    ));
}

fn call_dispatch_helper_id(
    descriptor: &crate::compiled_unit::NativeCallSiteDescriptor,
) -> &'static str {
    use crate::compiled_unit::NativeCallSiteKind;
    match descriptor.kind {
        NativeCallSiteKind::Function => "call_function",
        NativeCallSiteKind::Method => "call_method",
        NativeCallSiteKind::StaticMethod => "call_static_method",
        NativeCallSiteKind::Closure | NativeCallSiteKind::Callable | NativeCallSiteKind::Pipe => {
            "call_callable"
        }
        NativeCallSiteKind::Constructor | NativeCallSiteKind::DynamicConstructor => {
            "call_constructor"
        }
        NativeCallSiteKind::Semantic => "semantic_operation",
    }
}

fn native_dynamic_call_reason(
    context: &NativeRequestColdState<'_>,
    frame: &php_jit::JitNativeCallFrame,
    descriptor: &crate::compiled_unit::NativeCallSiteDescriptor,
    arguments: &[php_jit::JitNativeCallArgument],
) -> &'static str {
    if arguments
        .iter()
        .any(|argument| argument.flags.0 & php_jit::JitNativeArgFlags::NAMED.0 != 0)
    {
        return "named arguments";
    }
    if arguments
        .iter()
        .any(|argument| argument.flags.0 & php_jit::JitNativeArgFlags::UNPACK.0 != 0)
    {
        return "unpacked arguments";
    }
    if arguments
        .iter()
        .any(|argument| argument.flags.0 & php_jit::JitNativeArgFlags::BY_REFERENCE.0 != 0)
    {
        return "by-reference";
    }
    if matches!(
        descriptor.kind,
        crate::compiled_unit::NativeCallSiteKind::Method
            | crate::compiled_unit::NativeCallSiteKind::StaticMethod
    ) {
        return "method polymorphism";
    }
    if matches!(
        descriptor.kind,
        crate::compiled_unit::NativeCallSiteKind::Function
    ) && descriptor
        .target_symbol
        .as_deref()
        .is_some_and(|name| context.external_function(name).is_some())
    {
        return "cross-unit target";
    }
    if frame.target.function_id == u32::MAX {
        return "unknown target";
    }
    let function = php_ir::FunctionId::new(frame.target.function_id);
    let Some(target) = context.unit.functions.get(function.index()) else {
        return "target not published";
    };
    if target
        .params
        .iter()
        .any(|parameter| parameter.type_.is_some())
    {
        return "typed parameters";
    }
    if target.params.iter().any(|parameter| parameter.by_ref) || target.returns_by_ref {
        return "by-reference";
    }
    if target.params.iter().any(|parameter| parameter.variadic) {
        return "variadic target";
    }
    if arguments.len() < target.params.len() {
        let omitted = &target.params[arguments.len()..];
        if omitted
            .iter()
            .any(|parameter| matches!(parameter.default, Some(php_ir::IrConstant::Array(_))))
        {
            return "omitted array defaults";
        }
        if omitted.iter().any(|parameter| parameter.default.is_none()) {
            return "omitted required arguments";
        }
        if omitted.iter().any(|parameter| {
            parameter.default.as_ref().is_some_and(|default| {
                !context
                    .unit
                    .constants
                    .iter()
                    .any(|constant| constant == default)
            })
        }) {
            return "omitted non-interned scalar defaults";
        }
        return "omitted interned scalar defaults";
    }
    if arguments.len() > target.params.len() {
        return "extra positional arguments";
    }
    if target.flags.is_closure || !target.captures.is_empty() {
        return "closure/capture";
    }
    if target.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            matches!(
                instruction.kind,
                php_ir::InstructionKind::EnterTry { .. } | php_ir::InstructionKind::Throw { .. }
            )
        })
    }) {
        return "exception metadata";
    }
    "signature mismatch"
}

fn mark_native_function_argument_references(
    arguments: &mut [php_jit::JitNativeCallArgument],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    parameters: &[php_ir::IrParam],
) {
    let variadic_index = parameters.iter().position(|parameter| parameter.variadic);
    let fixed_count = variadic_index.unwrap_or(parameters.len());
    let mut positional = 0usize;
    let mut inline_assigned = [false; 64];
    let mut overflow_assigned = (fixed_count > inline_assigned.len())
        .then(|| vec![false; fixed_count.saturating_sub(inline_assigned.len())]);

    for (index, argument) in arguments.iter_mut().enumerate() {
        let call_argument = metadata.and_then(|metadata| metadata.get(index));
        let named_index = call_argument
            .and_then(|argument| argument.name.as_deref())
            .and_then(|name| {
                parameters[..fixed_count]
                    .iter()
                    .position(|parameter| parameter.name == name)
            });
        if let Some(index) = named_index {
            if index < inline_assigned.len() {
                inline_assigned[index] = true;
            } else if let Some(values) = overflow_assigned.as_mut()
                && let Some(value) = values.get_mut(index - inline_assigned.len())
            {
                *value = true;
            }
        }
        let parameter_index = named_index.or_else(|| {
            while positional < fixed_count
                && if positional < inline_assigned.len() {
                    inline_assigned[positional]
                } else {
                    overflow_assigned
                        .as_ref()
                        .and_then(|values| values.get(positional - inline_assigned.len()))
                        .copied()
                        .unwrap_or(false)
                }
            {
                positional += 1;
            }
            if positional < fixed_count {
                let index = positional;
                if index < inline_assigned.len() {
                    inline_assigned[index] = true;
                } else if let Some(values) = overflow_assigned.as_mut()
                    && let Some(value) = values.get_mut(index - inline_assigned.len())
                {
                    *value = true;
                }
                positional += 1;
                Some(index)
            } else {
                variadic_index
            }
        });
        let parameter = parameter_index.and_then(|index| parameters.get(index));
        let requires_reference = !call_argument.is_some_and(|argument| argument.unpack)
            && parameter.is_some_and(|parameter| parameter.by_ref);
        if requires_reference {
            argument.flags.0 |= php_jit::JitNativeArgFlags::BY_REFERENCE.0;
        } else {
            argument.flags.0 &= !php_jit::JitNativeArgFlags::BY_REFERENCE.0;
        }
    }
}

/// Resolve one call argument against the now-published userland signature.
/// The caller may have been compiled before an included function was declared,
/// so this is the last point where an unresolved lvalue can avoid being
/// needlessly converted into a PHP reference.
pub(super) fn native_function_argument_requires_reference_at(
    metadata: &[php_ir::instruction::IrCallArg],
    parameters: &[php_ir::IrParam],
    target_argument: usize,
) -> Option<bool> {
    let variadic_index = parameters.iter().position(|parameter| parameter.variadic);
    let fixed_count = variadic_index.unwrap_or(parameters.len());
    let mut positional = 0usize;
    let mut inline_assigned = [false; 64];
    let mut overflow_assigned = (fixed_count > inline_assigned.len())
        .then(|| vec![false; fixed_count.saturating_sub(inline_assigned.len())]);

    for (index, call_argument) in metadata.iter().enumerate().take(target_argument + 1) {
        let named_index = call_argument.name.as_deref().and_then(|name| {
            parameters[..fixed_count]
                .iter()
                .position(|parameter| parameter.name == name)
        });
        if let Some(index) = named_index {
            if index < inline_assigned.len() {
                inline_assigned[index] = true;
            } else if let Some(values) = overflow_assigned.as_mut()
                && let Some(value) = values.get_mut(index - inline_assigned.len())
            {
                *value = true;
            }
        }
        let parameter_index = named_index.or_else(|| {
            while positional < fixed_count
                && if positional < inline_assigned.len() {
                    inline_assigned[positional]
                } else {
                    overflow_assigned
                        .as_ref()
                        .and_then(|values| values.get(positional - inline_assigned.len()))
                        .copied()
                        .unwrap_or(false)
                }
            {
                positional += 1;
            }
            if positional < fixed_count {
                let index = positional;
                if index < inline_assigned.len() {
                    inline_assigned[index] = true;
                } else if let Some(values) = overflow_assigned.as_mut()
                    && let Some(value) = values.get_mut(index - inline_assigned.len())
                {
                    *value = true;
                }
                positional += 1;
                Some(index)
            } else {
                variadic_index
            }
        });
        if index == target_argument {
            return Some(
                !call_argument.unpack
                    && parameter_index
                        .and_then(|index| parameters.get(index))
                        .is_some_and(|parameter| parameter.by_ref),
            );
        }
    }
    None
}

// SAFETY: generated code owns the argument/local tables and result record for
// the complete synchronous helper invocation. Published callsite metadata
// validates the stable builtin ID before PHP-visible work begins.
#[allow(unsafe_code)] // Safety: the active cold request owns the raw VM state for this synchronous continuation.
pub(in crate::vm) extern "C" fn jit_baseline_native_builtin_dispatch_abi(
    runtime: *mut NativeRequestFastState,
    builtin_id: u32,
    function: u32,
    source_file: u32,
    source_start: u32,
    source_end: u32,
    arguments: *const i64,
    argument_count: u32,
    local_slots: *const php_jit::JitAbiSlot,
    local_count: u32,
    transition_state: *mut php_jit::JitDeoptState,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    // SAFETY: production publication validates the immutable callsite and
    // generated argument shape before this entry can execute.
    unsafe {
        jit_baseline_native_builtin_dispatch_impl::<false>(
            runtime,
            builtin_id,
            function,
            source_file,
            source_start,
            source_end,
            arguments,
            argument_count,
            local_slots,
            local_count,
            transition_state,
            out,
        )
    }
}

// SAFETY: diagnostic publication uses the same trusted internal ABI and adds
// accounting in a separately compiled function.
#[allow(unsafe_code)] // Safety: the active cold request owns the raw VM state for this synchronous continuation.
pub(in crate::vm) extern "C" fn jit_baseline_native_builtin_dispatch_diagnostic_abi(
    runtime: *mut NativeRequestFastState,
    builtin_id: u32,
    function: u32,
    source_file: u32,
    source_start: u32,
    source_end: u32,
    arguments: *const i64,
    argument_count: u32,
    local_slots: *const php_jit::JitAbiSlot,
    local_count: u32,
    transition_state: *mut php_jit::JitDeoptState,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    // SAFETY: diagnostic publication validates the same generated ABI.
    unsafe {
        jit_baseline_native_builtin_dispatch_impl::<true>(
            runtime,
            builtin_id,
            function,
            source_file,
            source_start,
            source_end,
            arguments,
            argument_count,
            local_slots,
            local_count,
            transition_state,
            out,
        )
    }
}

#[allow(unsafe_code)] // Safety: the active cold request owns the raw VM state for this synchronous continuation.
#[allow(clippy::too_many_arguments)]
unsafe fn jit_baseline_native_builtin_dispatch_impl<const DIAGNOSTIC: bool>(
    runtime: *mut NativeRequestFastState,
    builtin_id: u32,
    function: u32,
    source_file: u32,
    source_start: u32,
    source_end: u32,
    arguments: *const i64,
    argument_count: u32,
    local_slots: *const php_jit::JitAbiSlot,
    local_count: u32,
    transition_state: *mut php_jit::JitDeoptState,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    debug_assert!(!out.is_null());
    debug_assert!(argument_count == 0 || !arguments.is_null());
    debug_assert!(local_count == 0 || !local_slots.is_null());
    let arguments = if argument_count == 0 {
        &[]
    } else {
        // SAFETY: validated above; generated code publishes the exact length.
        unsafe { std::slice::from_raw_parts(arguments, argument_count as usize) }
    };
    let local_slots = if local_count == 0 {
        &[]
    } else {
        // SAFETY: validated above; generated code publishes the exact length.
        unsafe { std::slice::from_raw_parts(local_slots, local_count as usize) }
    };
    let callsite_span =
        php_ir::IrSpan::new(php_ir::FileId::new(source_file), source_start, source_end);
    let outcome = with_baseline_native_context_for(runtime, "call_dispatch", |context| {
        let prepared = crate::compiled_unit::PreparedNativeBuiltin::for_dense_id(
            builtin_id,
            argument_count as usize,
            true,
        )
        .ok_or_else(|| {
            format!("E_PHP_VM_UNRESOLVED_CALLABLE: builtin ID {builtin_id} is unavailable")
        })?;
        let entry = prepared.entry;
        let started_at = DIAGNOSTIC.then(std::time::Instant::now);
        if DIAGNOSTIC {
            let mut telemetry = context.runtime_telemetry.borrow_mut();
            telemetry.counters.native_call_direct =
                telemetry.counters.native_call_direct.saturating_add(1);
            telemetry.counters.native_builtin_direct_eligible = telemetry
                .counters
                .native_builtin_direct_eligible
                .saturating_add(1);
            telemetry.counters.native_builtin_direct_executed = telemetry
                .counters
                .native_builtin_direct_executed
                .saturating_add(1);
            telemetry.counters.native_callsite_total =
                telemetry.counters.native_callsite_total.saturating_add(1);
            telemetry.counters.native_call_frame_bytes = telemetry
                .counters
                .native_call_frame_bytes
                .saturating_add(std::mem::size_of_val(arguments) as u64)
                .saturating_add(std::mem::size_of_val(local_slots) as u64);
            let count = telemetry
                .counters
                .native_builtin_calls_by_name
                .entry(prepared.entry.name().to_owned())
                .or_default();
            *count = count.saturating_add(1);
            drop(telemetry);
            context.enter_runtime_helper("call_builtin_direct");
        }
        // This entry is emitted only for positional, non-unpacked arguments.
        // Publication prepared the exact builtin record together with the
        // callsite, so the warm path neither rebinds arguments nor validates
        // a redundant helper ID/name pair.
        if DIAGNOSTIC {
            context.enter_builtin_attribution(entry.name());
        }
        let result = if matches!(
            entry.execution_kind(),
            php_runtime::api::BuiltinExecutionKind::Runtime
        ) && !matches!(
            entry.name(),
            "set_time_limit" | "strlen" | "count" | "sizeof" | "is_callable"
        ) {
            execute_baseline_prepared_runtime_builtin(context, arguments, callsite_span, prepared)
                .map_err(NativeCallControl::from_baseline_error)
        } else {
            let instruction = php_ir::Instruction {
                id: php_ir::InstrId::new(0),
                span: callsite_span,
                kind: php_ir::InstructionKind::Nop,
            };
            execute_baseline_native_builtin_control(
                context,
                entry.name(),
                arguments,
                &instruction,
                Some((function, local_slots)),
                Some(prepared),
            )
        };
        if DIAGNOSTIC {
            context.exit_builtin_attribution(entry.name());
        }
        if DIAGNOSTIC {
            let elapsed = started_at
                .map(|started| started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64)
                .unwrap_or(0);
            let mut telemetry = context.runtime_telemetry.borrow_mut();
            let total = telemetry
                .counters
                .native_builtin_time_nanos_by_name
                .entry(prepared.entry.name().to_owned())
                .or_default();
            *total = total.saturating_add(elapsed);
            drop(telemetry);
            context.exit_runtime_helper("call_builtin_direct");
        }
        result
    });

    finish_native_dispatch_outcome(runtime, outcome, Some(callsite_span), transition_state, out)
}

// Converts one trusted internal dispatch outcome into the stable native
// control result. PHP-visible throw/suspend/exit semantics remain centralized
// here while individual prepared dispatchers avoid the generic call frame.
// SAFETY: `out` is owned by generated code for the synchronous helper call.
#[allow(unsafe_code)]
pub(super) fn finish_native_dispatch_outcome(
    runtime: *mut NativeRequestFastState,
    outcome: Option<NativeCallResult>,
    callsite_span: Option<php_ir::IrSpan>,
    transition_state: *mut php_jit::JitDeoptState,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    debug_assert!(!out.is_null());
    let (status, value) = match outcome {
        Some(Ok(value)) => (php_jit::JitCallStatus::RETURN, Some(value)),
        Some(Err(NativeCallControl::Rethrow)) => {
            let value = with_baseline_native_context_for(runtime, "call_dispatch", |context| {
                let mut throwable = context.take_pending_throwable()?;
                if let Some(span) = callsite_span {
                    throwable = native_throwable_with_call_source(context, throwable, span);
                }
                context.encode_baseline_value(throwable).ok()
            })
            .flatten();
            (php_jit::JitCallStatus::THROW, value)
        }
        Some(Err(NativeCallControl::Throw { class, message })) => {
            let value = with_baseline_native_context_for(runtime, "call_dispatch", |context| {
                callsite_span
                    .and_then(|span| {
                        encode_native_throwable_at(context, &class, &message, span).ok()
                    })
                    .or_else(|| encode_native_throwable(context, &class, &message).ok())
            })
            .flatten();
            (php_jit::JitCallStatus::THROW, value)
        }
        Some(Err(NativeCallControl::ArgumentCount {
            function,
            passed,
            required,
            target_span,
        })) => {
            let value = with_baseline_native_context_for(runtime, "call_dispatch", |context| {
                let source_span = callsite_span.unwrap_or(target_span);
                let message = native_argument_count_message(
                    context,
                    &function,
                    passed,
                    required,
                    source_span,
                );
                encode_native_throwable_at(context, "ArgumentCountError", &message, target_span)
                    .ok()
            })
            .flatten();
            (php_jit::JitCallStatus::THROW, value)
        }
        Some(Err(NativeCallControl::Propagate { status, value })) => (status, Some(value)),
        Some(Err(NativeCallControl::SuspendFiber { state })) => {
            if let Some(state) = state
                && !transition_state.is_null()
            {
                // SAFETY: generated code owns this state buffer for the
                // complete synchronous baseline-native dispatch.
                unsafe { transition_state.write(*state) };
            }
            let value = with_baseline_native_context_for(runtime, "call_dispatch", |context| {
                context.pending_fiber_suspension_value.take()
            })
            .flatten();
            (php_jit::JitCallStatus::SUSPEND_FIBER, value)
        }
        Some(Err(NativeCallControl::Exit(value))) => (php_jit::JitCallStatus::EXIT, Some(value)),
        Some(Err(NativeCallControl::PublishedRuntimeError)) => {
            (php_jit::JitCallStatus::RUNTIME_ERROR, None)
        }
        Some(Err(NativeCallControl::RuntimeError(message))) => {
            let _ = with_baseline_native_context_for(runtime, "call_dispatch", |context| {
                publish_native_call_diagnostic(context, message)
            });
            (php_jit::JitCallStatus::RUNTIME_ERROR, None)
        }
        None => (php_jit::JitCallStatus::COMPILE_REQUIRED, None),
    };
    let status_code = status.0 as i32;
    // SAFETY: `out` was checked and remains caller-owned for this invocation.
    unsafe {
        out.write(php_jit::JitCallResult {
            status,
            detail: status_code as u32,
            value: value.map_or_else(php_jit::JitAbiSlot::default, |value| php_jit::JitAbiSlot {
                tag: 3,
                flags: 0,
                payload: value as u64,
            }),
        });
    }
    status_code
}

/// Typed native call trampoline entry. Target compilation and lookup are
/// requested explicitly; this boundary has no alternate executor entry.
// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_baseline_native_call_dispatch_abi(
    runtime: *mut NativeRequestFastState,
    vm_context: u64,
    frame: *mut php_jit::JitNativeCallFrame,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    // SAFETY: production publication validates the generated frame ABI.
    unsafe { jit_baseline_native_call_dispatch_impl::<false>(runtime, vm_context, frame, out) }
}

// SAFETY: diagnostic publication uses the same generated ABI and adds
// accounting in a separately compiled entry.
#[allow(unsafe_code)] // Safety: the active cold request owns the raw VM state for this synchronous continuation.
pub(in crate::vm) extern "C" fn jit_baseline_native_call_dispatch_diagnostic_abi(
    runtime: *mut NativeRequestFastState,
    vm_context: u64,
    frame: *mut php_jit::JitNativeCallFrame,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    // SAFETY: diagnostic publication validates the same frame contract.
    unsafe { jit_baseline_native_call_dispatch_impl::<true>(runtime, vm_context, frame, out) }
}

#[allow(unsafe_code)] // Safety: the active cold request owns the raw VM state for this synchronous continuation.
unsafe fn jit_baseline_native_call_dispatch_impl<const DIAGNOSTIC: bool>(
    runtime: *mut NativeRequestFastState,
    _vm_context: u64,
    frame: *mut php_jit::JitNativeCallFrame,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    debug_assert!(!runtime.is_null());
    debug_assert!(!frame.is_null());
    debug_assert!(!out.is_null());
    if DIAGNOSTIC && (frame.is_null() || out.is_null()) {
        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    let result = {
        // SAFETY: The generated caller owns both records for this synchronous
        // call and the pointers were checked for null above.
        let frame = unsafe { &mut *frame };
        let compact_arguments =
            frame.flags & php_jit::JitNativeCallFrame::FLAG_COMPACT_ARGUMENTS != 0;
        let compact_argument_values: &[i64] = if compact_arguments && frame.argument_count != 0 {
            // SAFETY: The compact frame flag is emitted only with a contiguous
            // caller-owned i64 table containing exactly `argument_count`
            // entries for this synchronous call.
            unsafe {
                std::slice::from_raw_parts(
                    frame.arguments as *const i64,
                    frame.argument_count as usize,
                )
            }
        } else {
            &[]
        };
        let mut empty_arguments = [];
        let arguments: &mut [php_jit::JitNativeCallArgument] =
            if compact_arguments || frame.argument_count == 0 {
                &mut empty_arguments
            } else {
                // SAFETY: The native compiler emits the caller-owned argument
                // table and its exact live count. This internal hot ABI is trusted
                // after code publication instead of revalidating every call.
                unsafe {
                    std::slice::from_raw_parts_mut(
                        frame.arguments as *mut php_jit::JitNativeCallArgument,
                        frame.argument_count as usize,
                    )
                }
            };
        let mut callsite_span = None;
        let outcome = with_baseline_native_context_for(
            runtime,
            "call_dispatch",
            |context| -> NativeCallResult {
                let descriptor =
                    context.prepared_native_callsite(frame.function_id, frame.continuation_id);
                let Some(descriptor) = descriptor else {
                    return Err(format!(
                        "E_PHP_VM_UNRESOLVED_CALLABLE: native call site is unavailable at function={} block={} instruction={}",
                        frame.function_id, frame.source_block_id, frame.source_instruction_id,
                    )
                    .into());
                };
                // SAFETY: the descriptor is owned by the active compiled unit.
                // Unit storage remains alive and immutable for the synchronous
                // native dispatch, while the raw pointer avoids an atomic Arc
                // clone/drop on every warm callsite invocation.
                let descriptor = unsafe { &*descriptor };
                callsite_span = Some(descriptor.span);
                let instruction = descriptor.semantic_instruction();
                let direct_builtin =
                    frame.flags & php_jit::JitNativeCallFrame::FLAG_DIRECT_BUILTIN != 0;
                let direct_external =
                    frame.flags & php_jit::JitNativeCallFrame::FLAG_DIRECT_EXTERNAL != 0;
                let semantic_operation = if direct_builtin {
                    None
                } else {
                    semantic_operation_from_frame(frame)?
                };
                let direct_external_in_place = !direct_builtin
                    && matches!(
                        descriptor.kind,
                        crate::compiled_unit::NativeCallSiteKind::Function
                    )
                    && descriptor
                        .target_symbol
                        .as_deref()
                        .and_then(|name| context.external_function(name))
                        .is_some_and(|target| context.can_invoke_external_in_place(target));
                let (mut encoded, encoded_capacity_before) = if compact_arguments && direct_builtin
                {
                    // Compact direct calls already expose the exact payload
                    // slice consumed by the builtin. Borrow it in place instead
                    // of copying every argument through the runtime scratch Vec.
                    (std::borrow::Cow::Borrowed(compact_argument_values), 0)
                } else {
                    let mut encoded = std::mem::take(&mut context.native_call_encoded_scratch);
                    let encoded_capacity_before = encoded.capacity();
                    encoded.clear();
                    if compact_arguments {
                        encoded.extend_from_slice(compact_argument_values);
                    } else {
                        encoded.extend(
                            arguments
                                .iter()
                                .map(|argument| argument.value.payload as i64),
                        );
                    }
                    (std::borrow::Cow::Owned(encoded), encoded_capacity_before)
                };
                let empty_local_values = [];
                let local_values: &[php_jit::JitAbiSlot] = if frame.local_count == 0 {
                    &empty_local_values
                } else {
                    // SAFETY: ABI validation above proves a non-null caller-owned
                    // local table with `local_count` live entries. The generated
                    // caller stays suspended for this synchronous dispatch.
                    unsafe {
                        std::slice::from_raw_parts(
                            frame.local_slots as *const php_jit::JitAbiSlot,
                            frame.local_count as usize,
                        )
                    }
                };
                if DIAGNOSTIC {
                    let allocated_bytes = match &encoded {
                        std::borrow::Cow::Borrowed(_) => 0,
                        std::borrow::Cow::Owned(encoded) => encoded
                            .capacity()
                            .saturating_sub(encoded_capacity_before)
                            .saturating_mul(std::mem::size_of::<i64>()),
                    };
                    let mut telemetry = context.runtime_telemetry.borrow_mut();
                    if direct_builtin {
                        telemetry.counters.native_call_direct =
                            telemetry.counters.native_call_direct.saturating_add(1);
                        telemetry.counters.native_builtin_direct_eligible = telemetry
                            .counters
                            .native_builtin_direct_eligible
                            .saturating_add(1);
                        telemetry.counters.native_builtin_direct_executed = telemetry
                            .counters
                            .native_builtin_direct_executed
                            .saturating_add(1);
                        if let Some(entry) = descriptor.direct_builtin {
                            let count = telemetry
                                .counters
                                .native_builtin_calls_by_name
                                .entry(entry.entry.name().to_owned())
                                .or_default();
                            *count = count.saturating_add(1);
                        }
                    } else if direct_external_in_place {
                        telemetry.counters.native_call_direct =
                            telemetry.counters.native_call_direct.saturating_add(1);
                        telemetry.counters.native_cross_unit_direct_eligible = telemetry
                            .counters
                            .native_cross_unit_direct_eligible
                            .saturating_add(1);
                        telemetry.counters.native_cross_unit_direct_executed = telemetry
                            .counters
                            .native_cross_unit_direct_executed
                            .saturating_add(1);
                    } else if direct_external {
                        telemetry.counters.native_call_dynamic =
                            telemetry.counters.native_call_dynamic.saturating_add(1);
                        telemetry.counters.native_cross_unit_direct_eligible = telemetry
                            .counters
                            .native_cross_unit_direct_eligible
                            .saturating_add(1);
                    } else {
                        telemetry.counters.native_call_dynamic =
                            telemetry.counters.native_call_dynamic.saturating_add(1);
                    }
                    telemetry.counters.native_callsite_total =
                        telemetry.counters.native_callsite_total.saturating_add(1);
                    telemetry.counters.native_call_argument_allocation_bytes = telemetry
                        .counters
                        .native_call_argument_allocation_bytes
                        .saturating_add(allocated_bytes as u64);
                    telemetry.counters.native_call_frame_bytes =
                        telemetry.counters.native_call_frame_bytes.saturating_add(
                            (std::mem::size_of::<php_jit::JitNativeCallFrame>()
                                + if compact_arguments {
                                    std::mem::size_of_val(compact_argument_values)
                                } else {
                                    std::mem::size_of_val(arguments)
                                }) as u64,
                        );
                    drop(telemetry);
                    if !direct_builtin && !direct_external_in_place {
                        let dynamic_reason =
                            native_dynamic_call_reason(context, frame, descriptor, arguments);
                        let dynamic_target =
                            descriptor.target_symbol.as_deref().unwrap_or({
                                match descriptor.kind {
                            crate::compiled_unit::NativeCallSiteKind::Closure => "<closure>",
                            crate::compiled_unit::NativeCallSiteKind::Callable => "<callable>",
                            crate::compiled_unit::NativeCallSiteKind::Pipe => "<pipe>",
                            crate::compiled_unit::NativeCallSiteKind::DynamicConstructor => {
                                "<dynamic-constructor>"
                            }
                            crate::compiled_unit::NativeCallSiteKind::Semantic => "<semantic>",
                            crate::compiled_unit::NativeCallSiteKind::Function
                            | crate::compiled_unit::NativeCallSiteKind::Method
                            | crate::compiled_unit::NativeCallSiteKind::StaticMethod
                            | crate::compiled_unit::NativeCallSiteKind::Constructor => "<unknown>",
                        }
                            });
                        let mut telemetry = context.runtime_telemetry.borrow_mut();
                        let count = telemetry
                            .counters
                            .native_call_dynamic_by_reason
                            .entry(dynamic_reason.to_owned())
                            .or_default();
                        *count = count.saturating_add(1);
                        let target_count = telemetry
                            .counters
                            .native_call_dynamic_by_target
                            .entry(format!("{dynamic_reason}: {dynamic_target}"))
                            .or_default();
                        *target_count = target_count.saturating_add(1);
                    }
                }
                let helper_id = if direct_builtin {
                    "call_builtin_direct"
                } else if let Some(operation) = semantic_operation {
                    semantic_operation_helper_id(operation)
                } else {
                    call_dispatch_helper_id(descriptor)
                };
                if DIAGNOSTIC {
                    context.enter_runtime_helper(helper_id);
                }
                let callsite_started_at = DIAGNOSTIC.then(std::time::Instant::now);
                let outcome = (|| -> NativeCallResult {
            let completed_nested_fiber_matches = context
                .completed_nested_fiber_call
                .as_ref()
                .is_some_and(|(function, continuation, _, _)| {
                    *function == frame.function_id && *continuation == frame.continuation_id
                });
            if completed_nested_fiber_matches
                && let Some((_, _, status, value)) = context.completed_nested_fiber_call.take()
            {
                if status.is_terminal_return() {
                    return Ok(value);
                }
                return Err(NativeCallControl::Propagate { status, value });
            }
            if direct_external_in_place {
                let name = descriptor.target_symbol.as_deref().ok_or_else(|| {
                    "E_PHP_VM_UNRESOLVED_CALLABLE: prepared external function has no symbol"
                        .to_owned()
                })?;
                let target = context.external_function(name).ok_or_else(|| {
                    format!("E_PHP_VM_UNRESOLVED_CALLABLE: function {name} is no longer visible")
                })?;
                let metadata = Some(descriptor.arguments.as_ref());
                if let Some(parameters) = context
                    .dynamic_units
                    .get(target.unit)
                    .and_then(|unit| unit.compiled.unit().functions.get(target.function.index()))
                    .map(|function| function.params.as_slice())
                {
                    mark_native_function_argument_references(arguments, metadata, parameters);
                }
                bind_native_property_reference_arguments(
                    context,
                    arguments,
                    encoded.to_mut(),
                    metadata,
                )?;
                emit_native_external_deprecated_call(context, target, instruction);
                let returns_by_reference = context
                    .dynamic_units
                    .get(target.unit)
                    .and_then(|unit| {
                        unit.compiled
                            .unit()
                            .functions
                            .get(target.function.index())
                    })
                    .is_some_and(|function| function.returns_by_ref);
                emit_non_reference_return_binding_notice(
                    context,
                    instruction,
                    returns_by_reference,
                );
                return invoke_native_external_function_with_metadata(
                    context,
                    target,
                    &encoded,
                    metadata,
                    None,
                    context
                        .unit
                        .strict_types_for_span(descriptor.span),
                );
            }
            if direct_builtin {
                let entry = descriptor
                    .direct_builtin
                    .ok_or_else(|| {
                        format!(
                            "E_PHP_VM_UNRESOLVED_CALLABLE: builtin helper {} is not published",
                            frame.target.function_id
                        )
                    })?;
                let expanded = bind_native_builtin_arguments(
                    context,
                    entry.entry.name(),
                    &encoded,
                    Some(descriptor.arguments.as_ref()),
                )?;
                return execute_baseline_native_builtin_control(
                    context,
                    entry.entry.name(),
                    &expanded,
                    instruction,
                    Some((frame.function_id, local_values)),
                    Some(entry),
                );
            }
            if let Some(operation) = semantic_operation {
                return Ok(execute_native_semantic_operation(
                    context,
                    operation,
                    instruction,
                    &encoded,
                    frame.function_id,
                    frame.continuation_id,
                )?);
            }
            if let Some(result) =
                execute_native_static_property(context, instruction, &encoded, frame.function_id)
            {
                return Ok(result?);
            }
            if let Some(result) = execute_native_fiber_suspend(context, instruction, &encoded) {
                return result;
            }
            if let Some(result) = execute_baseline_instanceof(context, instruction, &encoded) {
                return Ok(result?);
            }
            if let Some(result) = execute_baseline_resolve_callable(context, instruction) {
                return Ok(result?);
            }
            if let Some(result) = execute_baseline_acquire_callable(context, instruction, &encoded) {
                return Ok(result?);
            }
            if matches!(
                instruction.kind,
                php_ir::InstructionKind::CallFunction { .. }
                    | php_ir::InstructionKind::CallCallable { .. }
                    | php_ir::InstructionKind::CallClosure { .. }
                    | php_ir::InstructionKind::Pipe { .. }
            ) && !direct_builtin
                && frame.target.function_id != u32::MAX
            {
                let function = php_ir::FunctionId::new(frame.target.function_id);
                emit_native_deprecated_call(context, function, instruction);
                let visible_arguments = encoded
                    .get(descriptor.argument_operand_offset..)
                    .ok_or_else(|| {
                        "E_PHP_VM_NATIVE_CALLSITE_MISMATCH: argument operand offset is stale"
                            .to_owned()
                    })?;
                let invocation_arguments = if descriptor.target_function.is_some() {
                    encoded.as_ref()
                } else {
                    visible_arguments
                };
                let metadata = match instruction.kind {
                    php_ir::InstructionKind::CallFunction { .. }
                    | php_ir::InstructionKind::CallCallable { .. }
                    | php_ir::InstructionKind::CallClosure { .. } => {
                        Some(descriptor.arguments.as_ref())
                    }
                    php_ir::InstructionKind::Pipe { .. } => None,
                    _ => None,
                };
                if native_function_is_generator(context, function) {
                    return create_baseline_bound_generator_with_metadata_strict(
                        context,
                        function,
                        invocation_arguments,
                        metadata,
                        context.unit.strict_types_for_span(descriptor.span),
                    );
                }
                return invoke_native_function_with_metadata_strict(
                    context,
                    function,
                    invocation_arguments,
                    metadata,
                    context.unit.strict_types_for_span(descriptor.span),
                );
            }
            if let Some(result) =
                execute_native_dynamic_constructor(context, instruction, &encoded)
            {
                return Ok(result?);
            }
            if frame.target.function_id == u32::MAX
                && frame.target.kind != php_jit::JitNativeCallKind::FUNCTION
                && let Some(result) =
                    execute_native_dynamic_callable(
                        context,
                        instruction,
                        &encoded,
                        Some(frame.function_id),
                        None,
                        false,
                    )
            {
                return result;
            }
            if let Some(result) = execute_native_property_instruction(
                context,
                instruction,
                &encoded,
                frame.function_id,
                None,
            ) {
                return Ok(result?);
            }
            if let Some(result) =
                execute_baseline_class_constant(context, instruction, frame.function_id)
            {
                return Ok(result?);
            }
            if let Some(result) = execute_native_internal_class(context, instruction, &encoded) {
                return Ok(result?);
            }
            if let Some(result) = execute_native_array_object(context, instruction, &encoded) {
                return Ok(result?);
            }
            if let Some(result) = execute_native_enum_static_method(context, instruction, &encoded)
            {
                return Ok(result?);
            }
            if let Some(result) = execute_native_generator_method(context, instruction, &encoded) {
                return result;
            }
            if let Some(result) = execute_native_fiber_method(context, instruction, &encoded) {
                return result;
            }
            if let php_ir::InstructionKind::CallMethod { method, .. } = &instruction.kind
                && method.eq_ignore_ascii_case("bindTo")
                && let Some(closure) = encoded.first().copied()
                && let Some(result) = context.rebind_prepared_closure(
                    closure,
                    encoded.get(1).copied(),
                    encoded.get(2).copied(),
                    "Closure::bindTo",
                )
            {
                return Ok(result?);
            }
            if let php_ir::InstructionKind::NewObject {
                display_class_name, ..
            } = &instruction.kind
                && display_class_name.eq_ignore_ascii_case("Fiber")
            {
                let callback = encoded
                    .first()
                    .copied()
                    .ok_or_else(|| "Fiber::__construct() expects a callback".to_owned())?;
                let callback = context.dereference_direct_encoding(callback);
                if context.native_encoded_value_kind(callback)
                    != Some(NativeEncodedValueKind::Callable)
                    || context.prepared_callable_dispatch(callback).is_none()
                {
                    return Err(
                        "Fiber::__construct(): Argument #1 ($callback) must be of type callable"
                            .into(),
                    );
                }
                return Ok(context.encode_native_fiber(callback)?);
            }
            if let php_ir::InstructionKind::NewObject {
                display_class_name, ..
            } = &instruction.kind
                // A known constructor call carries the already allocated
                // receiver as argument zero. Let the unified native-call path
                // below invoke that constructor; allocating and returning a
                // second object here would skip the constructor entirely.
                && frame.target.function_id == u32::MAX
            {
                let display_class_name = native_resolve_scoped_class_name(
                    context,
                    display_class_name,
                    frame.function_id,
                )?;
                let normalized = normalize_class_name(&display_class_name);
                if let Some(class) = native_active_class_handle(context, &normalized) {
                    ensure_native_class_parent_is_available(context, &class, instruction)?;
                    let mut parent = class.parent.clone();
                    let mut throwable_parent = false;
                    let mut visited = std::collections::BTreeSet::new();
                    while let Some(name) = parent.take() {
                        let name = normalize_class_name(&name);
                        if !visited.insert(name.clone()) {
                            break;
                        }
                        if (name.ends_with("exception") || name.ends_with("error"))
                            && (php_std::ExtensionRegistry::standard_library()
                                .enabled_class(&name)
                                .is_some()
                                || matches!(
                                    name.as_str(),
                                    "exception"
                                        | "error"
                                        | "typeerror"
                                        | "valueerror"
                                        | "argumentcounterror"
                                        | "fibererror"
                                ))
                        {
                            throwable_parent = true;
                            break;
                        }
                        parent = context
                            .unit
                            .classes
                            .iter()
                            .find(|candidate| candidate.name == name)
                            .and_then(|candidate| candidate.parent.clone())
                            .or_else(|| {
                                native_external_class_ref(context, &name)
                                    .and_then(|(_, candidate)| candidate.parent.clone())
                            });
                    }
                    if throwable_parent {
                        let message = encoded
                            .first()
                            .map(|message| context.decode_baseline_value(*message))
                            .transpose()?
                            .map(native_string)
                            .transpose()?
                            .map_or_else(String::new, |message| {
                                String::from_utf8_lossy(&message).into_owned()
                            });
                        return Ok(encode_native_throwable_at(
                            context,
                            &class.display_name,
                            &message,
                            instruction.span,
                        )?);
                    }
                    native_prepare_runtime_class_constants(
                        context,
                        None,
                        &class,
                        instruction,
                    )?;
                    let object = new_native_object(context, None, &class)?;
                    let receiver = context.encode_native_object_owner(object)?;
                    if let Some((constructor, _)) =
                        native_external_method(context, &class.name, "__construct")
                    {
                        if let Some(error) = native_external_method_access_error(
                            context,
                            constructor,
                            frame.function_id,
                            false,
                        ) {
                            return Err(format!("E_PHP_THROW:Error:{error}").into());
                        }
                        let mut constructor_arguments = Vec::with_capacity(encoded.len() + 1);
                        constructor_arguments.push(receiver);
                        constructor_arguments.extend_from_slice(&encoded);
                        let _ = invoke_native_external_function_with_metadata(
                            context,
                            constructor,
                            &constructor_arguments,
                            Some(descriptor.arguments.as_ref()),
                            Some(class.name.clone()),
                            context.unit.strict_types_for_span(instruction.span),
                        )?;
                    }
                    return Ok(receiver);
                }
                if !native_external_class_exists(context, &display_class_name)
                    && context.autoload_in_progress.insert(normalized.clone())
                {
                    let result = invoke_registered_autoload_callbacks_until(
                        context,
                        display_class_name.as_bytes(),
                        instruction,
                        |context| native_external_class_exists(context, &display_class_name),
                    );
                    context.autoload_in_progress.remove(&normalized);
                    result?;
                }
                if let Some((_, class)) =
                    native_external_class_handle(context, &display_class_name)
                {
                    if let Some(parent) = class
                        .parent_display_name
                        .clone()
                        .or_else(|| class.parent.clone())
                    {
                        native_autoload_class(context, &parent, instruction)?;
                    }
                    return Ok(create_native_external_object(
                        context,
                        &display_class_name,
                        &encoded,
                        instruction,
                    )?);
                }
            }
            if let php_ir::InstructionKind::CallMethod { method, .. } = &instruction.kind
                && (method.eq_ignore_ascii_case("getMessage")
                    || method.eq_ignore_ascii_case("getTrace")
                    || method.eq_ignore_ascii_case("getCode")
                    || method.eq_ignore_ascii_case("getPrevious"))
                && let Some(receiver) = encoded.first()
            {
                let decoded = context.decode_baseline_value(*receiver)?;
                let name = if method.eq_ignore_ascii_case("getTrace") {
                    "trace"
                } else if method.eq_ignore_ascii_case("getCode") {
                    "code"
                } else if method.eq_ignore_ascii_case("getPrevious") {
                    "previous"
                } else {
                    "message"
                };
                let fallback = if method.eq_ignore_ascii_case("getTrace") {
                    Value::Array(php_runtime::api::PhpArray::new())
                } else if method.eq_ignore_ascii_case("getCode") {
                    Value::Int(0)
                } else {
                    Value::Null
                };
                let value = match decoded {
                    Value::Array(exception) => {
                        let key = php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                            name.as_bytes().to_vec(),
                        ));
                        exception.get(&key).cloned().unwrap_or(fallback)
                    }
                    Value::Object(exception) => {
                        exception.get_property(name).unwrap_or(fallback)
                    }
                    value => {
                        let path = context
                            .unit
                            .files
                            .get(instruction.span.file.index())
                            .map_or("<unknown>", |file| file.path.as_str());
                        let line = native_source_line(context, instruction);
                        return Err(format!(
                            "Call to a member function {method}() on {} at {path}:{line}",
                            native_value_type_name(&value),
                        )
                        .into());
                    }
                };
                return Ok(context.encode_baseline_value(value)?);
            }
            if let php_ir::InstructionKind::BindReferenceFromMethodCall {
                method, args, ..
            } = &instruction.kind
                && let Some(receiver) = encoded.first()
            {
                let object = if let Some(object) = context.native_query_object(*receiver) {
                    object
                } else {
                    let receiver_value = match context.decode_baseline_value(*receiver).map_err(|error| {
                        format!("{method}() native receiver could not be decoded: {error}")
                    })? {
                        Value::Reference(reference) => reference.get(),
                        value => value,
                    };
                    let Value::Object(object) = receiver_value else {
                        return Err(format!(
                            "Call to a member function {method}() on {}",
                            native_value_type_name(&receiver_value),
                        )
                        .into());
                    };
                    object
                };
                let class_name = object.class_name();
                let local = native_calling_class(context, frame.function_id)
                    .and_then(|class| {
                        class
                            .methods
                            .iter()
                            .find(|entry| {
                                entry.name.eq_ignore_ascii_case(method) && entry.flags.is_private
                            })
                            .map(|entry| entry.function)
                    })
                    .or_else(|| native_method_in_hierarchy(context, &class_name, method));
                if let Some(function) = local {
                    if let Some(error) =
                        native_method_access_error(context, function, frame.function_id, false)
                    {
                        return Err(format!("E_PHP_THROW:Error:{error}").into());
                    }
                    let is_static = context.unit.classes.iter().any(|class| {
                        class
                            .methods
                            .iter()
                            .any(|entry| entry.function == function && entry.flags.is_static)
                    });
                    let arguments = if is_static { &encoded[1..] } else { &encoded };
                    return invoke_native_function_with_metadata_strict(
                        context,
                        function,
                        arguments,
                        Some(args),
                        context.unit.strict_types_for_span(instruction.span),
                    );
                }
                if let Some((function, entry)) =
                    native_external_method(context, &class_name, method)
                {
                    if let Some(error) = native_external_method_access_error(
                        context,
                        function,
                        frame.function_id,
                        false,
                    ) {
                        return Err(format!("E_PHP_THROW:Error:{error}").into());
                    }
                    let arguments = if entry.flags.is_static {
                        &encoded[1..]
                    } else {
                        &encoded
                    };
                    return invoke_native_external_function_with_metadata(
                        context,
                        function,
                        arguments,
                        Some(descriptor.arguments.as_ref()),
                        Some(class_name),
                        context
                            .unit
                            .strict_types_for_function(php_ir::FunctionId::new(frame.function_id)),
                    );
                }
            }
            if let php_ir::InstructionKind::CallMethod { method, args, .. } = &instruction.kind
                && let Some(receiver) = encoded.first()
            {
                let object = if let Some(object) = context.native_query_object(*receiver) {
                    // Method lookup needs only stable object/class identity.
                    // Keep declared slots authoritative in the native plane;
                    // decoding here previously rebuilt the receiver's entire
                    // nested property graph around every method call.
                    object
                } else {
                    let receiver_value = match context.decode_baseline_value(*receiver).map_err(|error| {
                        format!("{method}() native receiver could not be decoded: {error}")
                    })? {
                        Value::Reference(reference) => reference.get(),
                        value => value,
                    };
                    let Value::Object(object) = receiver_value else {
                        let path = context
                            .unit
                            .files
                            .get(instruction.span.file.index())
                            .map_or("<unknown>", |file| file.path.as_str());
                        let line = native_source_line(context, instruction);
                        return Err(format!(
                            "Call to a member function {method}() on {} at {path}:{line}",
                            native_value_type_name(&receiver_value),
                        )
                        .into());
                    };
                    object
                };
                let class_name = object.class_name();
                let receiver_layout_id = object.class_layout_epoch();
                if let Some(target) = context.lookup_native_method_pic(
                    descriptor,
                    &class_name,
                    receiver_layout_id,
                    method,
                )
                {
                    context.record_native_method_pic(true);
                    match target {
                        NativeMethodPicTarget::CurrentUnit {
                            function,
                            is_static,
                        } => {
                            emit_native_deprecated_call(context, function, instruction);
                            let call_arguments = if is_static { &encoded[1..] } else { &encoded };
                            if is_static {
                                context.called_classes.push(Arc::from(class_name.as_str()));
                            }
                            let result = invoke_native_function_with_metadata_strict(
                                context,
                                function,
                                call_arguments,
                                Some(descriptor.arguments.as_ref()),
                                context.unit.strict_types_for_span(descriptor.span),
                            );
                            if is_static {
                                context.called_classes.pop();
                            }
                            return result;
                        }
                        NativeMethodPicTarget::DynamicUnit {
                            function,
                            is_static,
                        } => {
                            let call_arguments = if is_static { &encoded[1..] } else { &encoded };
                            return invoke_native_external_function_with_metadata(
                                context,
                                function,
                                call_arguments,
                                Some(descriptor.arguments.as_ref()),
                                Some(class_name),
                                context.unit.strict_types_for_function(php_ir::FunctionId::new(
                                    frame.function_id,
                                )),
                            );
                        }
                    }
                }
                let function = native_calling_class(context, frame.function_id)
                    .and_then(|class| {
                        class
                            .methods
                            .iter()
                            .find(|entry| {
                                entry.name.eq_ignore_ascii_case(method) && entry.flags.is_private
                            })
                            .map(|entry| entry.function)
                    })
                    .or_else(|| native_method_in_hierarchy(context, &class_name, method));
                if let Some(function) = function {
                    emit_native_deprecated_call(context, function, instruction);
                    if let Some(error) =
                        native_method_access_error(context, function, frame.function_id, false)
                    {
                        if let Some(magic) =
                            native_method_in_hierarchy(context, &class_name, "__call")
                        {
                            let method_name = context.encode_native_string_owner(
                                PhpString::from_bytes(method.as_bytes().to_vec()),
                            )?;
                            let call_arguments =
                                encode_native_magic_call_arguments_array(context, &encoded[1..])?;
                            return invoke_native_function(
                                context,
                                magic,
                                &[*receiver, method_name, call_arguments],
                            );
                        }
                        return Err(format!("E_PHP_THROW:Error:{error}").into());
                    }
                    let is_static_method = context.unit.classes.iter().any(|class| {
                        class
                            .methods
                            .iter()
                            .any(|entry| entry.function == function && entry.flags.is_static)
                    });
                    let call_arguments = if is_static_method {
                        &encoded[1..]
                    } else {
                        &encoded
                    };
                    if native_function_is_generator(context, function) {
                        return create_baseline_bound_generator_with_metadata_strict(
                            context,
                            function,
                            call_arguments,
                            Some(args),
                            context.unit.strict_types_for_span(instruction.span),
                        );
                    }
                    if context.install_native_method_pic(
                        descriptor,
                        &class_name,
                        receiver_layout_id,
                        method,
                        NativeMethodPicTarget::CurrentUnit {
                            function,
                            is_static: is_static_method,
                        },
                    ) {
                        context.record_native_method_pic(false);
                    }
                    if is_static_method {
                        context.called_classes.push(Arc::from(class_name.as_str()));
                    }
                    let result = invoke_native_function_with_metadata_strict(
                        context,
                        function,
                        call_arguments,
                        Some(args),
                        context.unit.strict_types_for_span(instruction.span),
                    );
                    if is_static_method {
                        context.called_classes.pop();
                    }
                    return result;
                }
                if let Some((function, entry)) =
                    native_external_method(context, &class_name, method)
                {
                    if let Some(error) = native_external_method_access_error(
                        context,
                        function,
                        frame.function_id,
                        false,
                    ) {
                        return Err(format!("E_PHP_THROW:Error:{error}").into());
                    }
                    let call_arguments = if entry.flags.is_static {
                        &encoded[1..]
                    } else {
                        &encoded
                    };
                    if context.install_native_method_pic(
                        descriptor,
                        &class_name,
                        receiver_layout_id,
                        method,
                        NativeMethodPicTarget::DynamicUnit {
                            function,
                            is_static: entry.flags.is_static,
                        },
                    ) {
                        context.record_native_method_pic(false);
                    }
                    return invoke_native_external_function_with_metadata(
                        context,
                        function,
                        call_arguments,
                        Some(descriptor.arguments.as_ref()),
                        Some(class_name),
                        context
                            .unit
                            .strict_types_for_function(php_ir::FunctionId::new(frame.function_id)),
                    );
                }
                if let Some(function) = native_method_in_hierarchy(context, &class_name, "__call") {
                    let method_name = context.encode_native_string_owner(PhpString::from_bytes(
                        method.as_bytes().to_vec(),
                    ))?;
                    let call_arguments =
                        encode_native_magic_call_arguments_array(context, &encoded[1..])?;
                    return invoke_native_function(
                        context,
                        function,
                        &[*receiver, method_name, call_arguments],
                    );
                }
                if let Some((function, _entry)) =
                    native_external_method(context, &class_name, "__call")
                {
                    if let Some(error) = native_external_method_access_error(
                        context,
                        function,
                        frame.function_id,
                        false,
                    ) {
                        return Err(format!("E_PHP_THROW:Error:{error}").into());
                    }
                    let method_name = context.encode_native_string_owner(PhpString::from_bytes(
                        method.as_bytes().to_vec(),
                    ))?;
                    let call_arguments =
                        encode_native_magic_call_arguments_array(context, &encoded[1..])?;
                    return invoke_native_external_function(
                        context,
                        function,
                        &[*receiver, method_name, call_arguments],
                        Some(class_name),
                        context.unit.strict_types_for_span(instruction.span),
                    );
                }
            }
            let stable_static_target = match &instruction.kind {
                php_ir::InstructionKind::CallStaticMethod {
                    class_name,
                    method,
                    ..
                } => Some((class_name.as_str(), method.as_str())),
                _ if matches!(
                    descriptor.kind,
                    crate::compiled_unit::NativeCallSiteKind::StaticMethod
                ) => descriptor
                    .target_class
                    .as_deref()
                    .zip(descriptor.target_symbol.as_deref()),
                _ => None,
            };
            if let Some((class_name, method)) = stable_static_target {
                if class_name.eq_ignore_ascii_case("Closure") && method.eq_ignore_ascii_case("bind")
                {
                    let closure = encoded
                        .first()
                        .copied()
                        .ok_or_else(|| "Closure::bind() expects a closure".to_owned())?;
                    if let Some(result) = context.rebind_prepared_closure(
                        closure,
                        encoded.get(1).copied(),
                        encoded.get(2).copied(),
                        "Closure::bind",
                    ) {
                        return Ok(result?);
                    }
                    let closure = context.decode_baseline_value(closure)?;
                    let Value::Callable(callable) = closure else {
                        return Err("Closure::bind() expects a closure".into());
                    };
                    let php_runtime::api::CallableValue::Closure(closure) = callable.as_ref()
                    else {
                        return Err("Closure::bind() expects a closure".into());
                    };
                    let rebound = rebind_baseline_materialized_closure(
                        closure,
                        encoded
                            .get(1)
                            .copied()
                            .map(|value| context.decode_baseline_value(value))
                            .transpose()?,
                        encoded
                            .get(2)
                            .copied()
                            .map(|value| context.decode_baseline_value(value))
                            .transpose()?,
                    )?;
                    return Ok(context.encode_baseline_value(rebound)?);
                }
                let resolved_class = match class_name.to_ascii_lowercase().as_str() {
                    "self" => native_calling_class(context, frame.function_id)
                        .map(|class| class.name.clone()),
                    "static" => context
                        .called_classes
                        .last()
                        .map(|class| class.to_string())
                        .or_else(|| {
                        native_calling_class(context, frame.function_id)
                            .map(|class| class.name.clone())
                    }),
                    "parent" => native_calling_class(context, frame.function_id)
                        .and_then(|class| class.parent.clone()),
                    _ => Some(class_name.to_owned()),
                };
                if let Some(class) = resolved_class.as_deref() {
                    native_autoload_class(context, class, instruction)?;
                }
                if let Some(result) = resolved_class.as_deref().and_then(|class| {
                    initialize_native_throwable_parent(context, class, method, &encoded)
                }) {
                    return Ok(result?);
                }
                if let Some(class) = resolved_class.as_deref()
                    && let Some(target) =
                        context.lookup_native_method_pic(descriptor, class, 0, method)
                {
                    context.record_native_method_pic(true);
                    let forwarding = matches!(
                        class_name.to_ascii_lowercase().as_str(),
                        "self" | "parent" | "static"
                    );
                    let called_class = if forwarding {
                        context
                            .called_classes
                            .last()
                            .map(|class| class.to_string())
                            .or_else(|| resolved_class.clone())
                    } else {
                        resolved_class.clone()
                    };
                    match target {
                        NativeMethodPicTarget::CurrentUnit {
                            function,
                            is_static: true,
                        } => {
                            emit_native_deprecated_call(context, function, instruction);
                            let pushed_called_class = called_class.is_some();
                            if let Some(called_class) = called_class {
                                context.called_classes.push(Arc::from(called_class));
                            }
                            let result = invoke_native_function_with_metadata_strict(
                                context,
                                function,
                                &encoded,
                                Some(descriptor.arguments.as_ref()),
                                context.unit.strict_types_for_span(descriptor.span),
                            );
                            if pushed_called_class {
                                context.called_classes.pop();
                            }
                            return result;
                        }
                        NativeMethodPicTarget::DynamicUnit {
                            function,
                            is_static: true,
                        } => {
                            return invoke_native_external_function_with_metadata(
                                context,
                                function,
                                &encoded,
                                Some(descriptor.arguments.as_ref()),
                                Some(class.to_owned()),
                                context.unit.strict_types_for_function(php_ir::FunctionId::new(
                                    frame.function_id,
                                )),
                            );
                        }
                        NativeMethodPicTarget::CurrentUnit {
                            is_static: false, ..
                        }
                        | NativeMethodPicTarget::DynamicUnit {
                            is_static: false, ..
                        } => {}
                    }
                }
                let function = resolved_class
                    .as_deref()
                    .and_then(|class| native_method_in_hierarchy(context, class, method));
                if let Some(function) = function {
                    emit_native_deprecated_call(context, function, instruction);
                    if let Some(error) = native_method_access_error(
                        context,
                        function,
                        frame.function_id,
                        class_name.eq_ignore_ascii_case("static"),
                    ) {
                        if let Some(class) = resolved_class.as_deref()
                            && let Some(magic) =
                                native_method_in_hierarchy(context, class, "__callStatic")
                        {
                            let method_name = context.encode_native_string_owner(
                                PhpString::from_bytes(method.as_bytes().to_vec()),
                            )?;
                            let call_arguments =
                                encode_native_magic_call_arguments_array(context, &encoded)?;
                            return invoke_native_function(
                                context,
                                magic,
                                &[method_name, call_arguments],
                            );
                        }
                        return Err(format!("E_PHP_THROW:Error:{error}").into());
                    }
                    let is_instance_method = context.unit.classes.iter().any(|class| {
                        class
                            .methods
                            .iter()
                            .any(|entry| entry.function == function && !entry.flags.is_static)
                    });
                    if is_instance_method && frame.receiver_handle == 0 {
                        return Err(format!(
                            "Non-static method {}::{}() cannot be called statically",
                            resolved_class.as_deref().unwrap_or(class_name),
                            method
                        )
                        .into());
                    }
                    let forwarding = matches!(
                        class_name.to_ascii_lowercase().as_str(),
                        "self" | "parent" | "static"
                    );
                    let called_class = if forwarding {
                        context
                            .called_classes
                            .last()
                            .map(|class| class.to_string())
                            .or_else(|| resolved_class.clone())
                    } else {
                        resolved_class.clone()
                    };
                    let pushed_called_class = called_class.is_some();
                    if let Some(called_class) = called_class {
                        context.called_classes.push(Arc::from(called_class));
                    }
                    let result = if is_instance_method {
                        let mut call_arguments =
                            Vec::with_capacity(encoded.len().saturating_add(1));
                        call_arguments.push(frame.receiver_handle as i64);
                        call_arguments.extend_from_slice(&encoded);
                        invoke_native_function_with_metadata_strict(
                            context,
                            function,
                            &call_arguments,
                            Some(descriptor.arguments.as_ref()),
                            context.unit.strict_types_for_span(descriptor.span),
                        )
                    } else {
                        if let Some(class) = resolved_class.as_deref()
                            && context.install_native_method_pic(
                                descriptor,
                                class,
                                0,
                                method,
                                NativeMethodPicTarget::CurrentUnit {
                                    function,
                                    is_static: true,
                                },
                            )
                        {
                            context.record_native_method_pic(false);
                        }
                        invoke_native_function_with_metadata_strict(
                            context,
                            function,
                            &encoded,
                            Some(descriptor.arguments.as_ref()),
                            context.unit.strict_types_for_span(descriptor.span),
                        )
                    };
                    if pushed_called_class {
                        context.called_classes.pop();
                    }
                    return result;
                }
                if let Some(class) = resolved_class.as_deref()
                    && let Some((function, entry)) = native_external_method(context, class, method)
                {
                    if let Some(error) = native_external_method_access_error(
                        context,
                        function,
                        frame.function_id,
                        class_name.eq_ignore_ascii_case("static"),
                    ) {
                        return Err(format!("E_PHP_THROW:Error:{error}").into());
                    }
                    if !entry.flags.is_static && frame.receiver_handle == 0 {
                        return Err(format!(
                            "Non-static method {class}::{method}() cannot be called statically"
                        )
                        .into());
                    }
                    let result = if !entry.flags.is_static {
                        let mut call_arguments =
                            Vec::with_capacity(encoded.len().saturating_add(1));
                        call_arguments.push(frame.receiver_handle as i64);
                        call_arguments.extend_from_slice(&encoded);
                        invoke_native_external_function_with_metadata(
                            context,
                            function,
                            &call_arguments,
                            Some(descriptor.arguments.as_ref()),
                            Some(class.to_owned()),
                            context.unit.strict_types_for_function(php_ir::FunctionId::new(
                                frame.function_id,
                            )),
                        )
                    } else {
                        if context.install_native_method_pic(
                            descriptor,
                            class,
                            0,
                            method,
                            NativeMethodPicTarget::DynamicUnit {
                                function,
                                is_static: true,
                            },
                        ) {
                            context.record_native_method_pic(false);
                        }
                        invoke_native_external_function_with_metadata(
                            context,
                            function,
                            &encoded,
                            Some(descriptor.arguments.as_ref()),
                            Some(class.to_owned()),
                            context.unit.strict_types_for_function(php_ir::FunctionId::new(
                                frame.function_id,
                            )),
                        )
                    };
                    return result;
                }
                if let Some(class) = resolved_class {
                    let method_name = context.encode_native_string_owner(PhpString::from_bytes(
                        method.as_bytes().to_vec(),
                    ))?;
                    let call_arguments =
                        encode_native_magic_call_arguments_array(context, &encoded)?;
                    if let Some(function) =
                        native_method_in_hierarchy(context, &class, "__callStatic")
                    {
                        context.called_classes.push(Arc::from(class.as_str()));
                        let result = invoke_native_function(
                            context,
                            function,
                            &[method_name, call_arguments],
                        );
                        context.called_classes.pop();
                        return result;
                    }
                    if let Some((function, _entry)) =
                        native_external_method(context, &class, "__callStatic")
                    {
                        if let Some(error) = native_external_method_access_error(
                            context,
                            function,
                            frame.function_id,
                            false,
                        ) {
                            return Err(format!("E_PHP_THROW:Error:{error}").into());
                        }
                        return invoke_native_external_function(
                            context,
                            function,
                            &[method_name, call_arguments],
                            Some(class),
                            context.unit.strict_types_for_span(instruction.span),
                        );
                    }
                }
            }
            if let php_ir::InstructionKind::NewObject { args, .. } = &instruction.kind
                && frame.target.function_id != u32::MAX
            {
                let constructor = php_ir::FunctionId::new(frame.target.function_id);
                if let Some(error) =
                    native_method_access_error(context, constructor, frame.function_id, false)
                {
                    // PHP's constructor visibility diagnostic omits the word
                    // "method", unlike an ordinary inaccessible method call.
                    let error = error
                        .replace("private method ", "private ")
                        .replace("protected method ", "protected ");
                    return Err(format!("E_PHP_THROW:Error:{error}").into());
                }
                return invoke_native_function_with_metadata_strict(
                    context,
                    constructor,
                    &encoded,
                    Some(args),
                    context.unit.strict_types_for_span(instruction.span),
                );
            }
            let prepared_callable_symbol = matches!(
                instruction.kind,
                php_ir::InstructionKind::CallCallable { .. }
                    | php_ir::InstructionKind::Pipe { .. }
            )
            .then(|| descriptor.target_symbol.as_deref())
            .flatten()
            .map(std::borrow::Cow::Borrowed);
            let name = if let Some(name) = prepared_callable_symbol {
                Some(name)
            } else {
                match &instruction.kind {
                    php_ir::InstructionKind::CallFunction { name, .. }
                    | php_ir::InstructionKind::BindReferenceFromCall { name, .. } => {
                        Some(std::borrow::Cow::Borrowed(
                            descriptor.target_symbol.as_deref().unwrap_or(name.as_str()),
                        ))
                    }
                    php_ir::InstructionKind::NewObject {
                        display_class_name, ..
                    } => {
                        let display_class_name = native_resolve_scoped_class_name(
                            context,
                            display_class_name,
                            frame.function_id,
                        )?;
                        return Err(format!(
                            "E_PHP_VM_UNKNOWN_CLASS: Class {display_class_name} not found"
                        )
                        .into());
                    }
                    php_ir::InstructionKind::Pipe {
                    callable: php_ir::Operand::Register(callable),
                    ..
                } => {
                    let function = context.unit.functions.get(frame.function_id as usize);
                    let resolved = function.and_then(|function| {
                        function
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .find_map(|candidate| match &candidate.kind {
                                php_ir::InstructionKind::ResolveCallable {
                                    dst,
                                    callable:
                                        php_ir::instruction::CallableKind::FunctionName { name },
                                } if dst == callable => Some(name.clone()),
                                _ => None,
                            })
                    });
                    if resolved.is_none() {
                        let value =
                            function.and_then(|function| {
                                function
                                    .blocks
                                    .iter()
                                    .flat_map(|block| &block.instructions)
                                    .find_map(|candidate| match &candidate.kind {
                                        php_ir::InstructionKind::LoadConst { dst, constant }
                                            if dst == callable =>
                                        {
                                            context.unit.constants.get(constant.index()).and_then(
                                                |constant| ir_constant_value(constant).ok(),
                                            )
                                        }
                                        _ => None,
                                    })
                            });
                        return Err(format!(
                            "{} is not callable",
                            value.as_ref().map_or("value", native_value_type_name)
                        )
                        .into());
                    }
                    resolved.map(std::borrow::Cow::Owned)
                }
                php_ir::InstructionKind::Pipe { callable, .. } => {
                    let value = match callable {
                        php_ir::Operand::Constant(constant) => context
                            .unit
                            .constants
                            .get(constant.index())
                            .and_then(|constant| ir_constant_value(constant).ok()),
                        _ => None,
                    };
                    return Err(format!(
                        "{} is not callable",
                        value.as_ref().map_or("value", native_value_type_name)
                    )
                    .into());
                }
                php_ir::InstructionKind::CallCallable {
                    callee: php_ir::Operand::Register(callable),
                    ..
                } => context
                    .unit
                    .functions
                    .get(frame.function_id as usize)
                    .and_then(|function| {
                        function
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .find_map(|candidate| match &candidate.kind {
                                php_ir::InstructionKind::ResolveCallable {
                                    dst,
                                    callable:
                                        php_ir::instruction::CallableKind::FunctionName { name },
                                } if dst == callable => Some(name.clone()),
                                _ => None,
                            })
                    })
                    .map(std::borrow::Cow::Owned),
                php_ir::InstructionKind::CallMethod { method, .. } => {
                    let class_name = encoded
                        .first()
                        .and_then(|receiver| context.decode_baseline_value(*receiver).ok())
                        .and_then(|receiver| match receiver {
                            Value::Reference(reference) => match reference.get() {
                                Value::Object(object) => Some(object.class_name()),
                                _ => None,
                            },
                            Value::Object(object) => Some(object.class_name()),
                            _ => None,
                        })
                        .unwrap_or_else(|| "object".to_owned());
                    return Err(format!(
                        "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not implemented"
                    )
                    .into());
                }
                php_ir::InstructionKind::CallStaticMethod {
                    class_name, method, ..
                } => {
                    let class_name = native_resolve_scoped_class_name(
                        context,
                        class_name,
                        frame.function_id,
                    )?;
                    return Err(format!(
                        "E_PHP_VM_UNKNOWN_METHOD: static method {class_name}::{method} is not implemented"
                    )
                    .into());
                }
                php_ir::InstructionKind::BindReferenceFromMethodCall { method, .. } => {
                    return Err(format!(
                        "E_PHP_VM_UNKNOWN_METHOD: by-reference method call {method}() is not implemented"
                    )
                    .into());
                }
                    _ => None,
                }
            };
            let Some(name) = name else {
                return Err(format!(
                    "E_PHP_VM_UNRESOLVED_CALLABLE: native call target is unresolved for {:?} at function={} block={} instruction={} target_kind={} target_function={}",
                    instruction.kind,
                    frame.function_id,
                    frame.source_block_id,
                    frame.source_instruction_id,
                    frame.target.kind.0,
                    frame.target.function_id,
                )
                .into());
            };
            if matches!(
                instruction.kind,
                php_ir::InstructionKind::CallCallable { .. }
                    | php_ir::InstructionKind::Pipe { .. }
            ) && context.function_id(name.as_ref()).is_none()
                && context.external_function(name.as_ref()).is_none()
                && php_std::arginfo::function_metadata_indexed(name.as_ref()).is_none()
            {
                return Err(format!(
                    "E_PHP_THROW:Error:Call to undefined function {name}()"
                )
                .into());
            }
            let local_function_id = (!direct_builtin
                && matches!(
                    descriptor.kind,
                    crate::compiled_unit::NativeCallSiteKind::Function
                )
                && frame.target.function_id != u32::MAX)
                .then(|| php_ir::FunctionId::new(frame.target.function_id))
                .filter(|function| context.unit.functions.get(function.index()).is_some())
                .or_else(|| (!direct_builtin).then(|| context.function_id(name.as_ref())).flatten());
            if let Some(function_id) = local_function_id {
                emit_native_deprecated_call(context, function_id, instruction);
                let returns_by_reference = context
                    .unit
                    .functions
                    .get(function_id.index())
                    .is_some_and(|function| function.returns_by_ref);
                emit_non_reference_return_binding_notice(
                    context,
                    instruction,
                    returns_by_reference,
                );
                if let php_ir::InstructionKind::CallFunction { args, .. } = &instruction.kind
                    && let Some(target) = context.unit.functions.get(function_id.index())
                {
                    let required = target
                        .params
                        .iter()
                        .take_while(|parameter| parameter.required)
                        .count();
                    if args.len() < required {
                        let path = context
                            .unit
                            .files
                            .get(instruction.span.file.index())
                            .map_or("<unknown>", |file| file.path.as_str());
                        let line = native_source_line(context, instruction);
                        return Err(format!(
                            "E_PHP_THROW:ArgumentCountError:Too few arguments to function {}(), {} passed in {} on line {} and exactly {} expected",
                            target.name,
                            args.len(),
                            path,
                            line,
                            required
                        )
                        .into());
                    }
                }
                if context
                    .unit
                    .functions
                    .get(function_id.index())
                    .is_some_and(|function| {
                        function.flags.is_generator
                            || function
                                .blocks
                                .iter()
                                .flat_map(|block| &block.instructions)
                                .any(|instruction| {
                                    matches!(
                                        instruction.kind,
                                        php_ir::InstructionKind::Yield { .. }
                                            | php_ir::InstructionKind::YieldFrom { .. }
                                    )
                                })
                    })
                {
                    let metadata = Some(descriptor.arguments.as_ref());
                    if let Some(parameters) = context
                        .unit
                        .functions
                        .get(function_id.index())
                        .map(|function| function.params.as_slice())
                    {
                        mark_native_function_argument_references(arguments, metadata, parameters);
                    }
                    bind_native_property_reference_arguments(
                        context,
                        arguments,
                        encoded.to_mut(),
                        metadata,
                    )?;
                    return create_baseline_bound_generator_with_metadata_strict(
                        context,
                        function_id,
                        &encoded,
                        metadata,
                        context.unit.strict_types_for_span(descriptor.span),
                    );
                }
                let metadata = Some(descriptor.arguments.as_ref());
                if let Some(parameters) = context
                    .unit
                    .functions
                    .get(function_id.index())
                    .map(|function| function.params.as_slice())
                {
                    mark_native_function_argument_references(
                        arguments,
                        metadata,
                        parameters,
                    );
                }
                bind_native_property_reference_arguments(
                    context,
                    arguments,
                    encoded.to_mut(),
                    metadata,
                )?;
                return invoke_native_function_with_metadata_strict(
                    context,
                    function_id,
                    &encoded,
                    metadata,
                    context.unit.strict_types_for_span(descriptor.span),
                );
            }
            let metadata = matches!(
                descriptor.kind,
                crate::compiled_unit::NativeCallSiteKind::Function
            )
            .then_some(descriptor.arguments.as_ref());
            if !direct_builtin && let Some(function) = context.external_function(name.as_ref()) {
                if let Some(parameters) = context
                    .dynamic_units
                    .get(function.unit)
                    .and_then(|unit| unit.compiled.unit().functions.get(function.function.index()))
                    .map(|function| function.params.as_slice())
                {
                    mark_native_function_argument_references(arguments, metadata, parameters);
                }
                bind_native_property_reference_arguments(
                    context,
                    arguments,
                    encoded.to_mut(),
                    metadata,
                )?;
                emit_native_external_deprecated_call(context, function, instruction);
                let returns_by_reference = context
                    .dynamic_units
                    .get(function.unit)
                    .and_then(|unit| {
                        unit.compiled
                            .unit()
                            .functions
                            .get(function.function.index())
                    })
                    .is_some_and(|target| target.returns_by_ref);
                emit_non_reference_return_binding_notice(
                    context,
                    instruction,
                    returns_by_reference,
                );
                return invoke_native_external_function_with_metadata(
                    context,
                    function,
                    &encoded,
                    metadata,
                    None,
                    context.unit.strict_types_for_span(descriptor.span),
                );
            }
            if direct_external {
                return Err(format!(
                    "E_PHP_VM_UNRESOLVED_CALLABLE: published cross-unit target {name} is unavailable"
                )
                .into());
            }
            let builtin_name = if php_std::arginfo::function_metadata_indexed(name.as_ref())
                .is_some()
            {
                name.as_ref()
            } else {
                name.rsplit('\\').next().unwrap_or(name.as_ref())
            };
            let expanded =
                bind_native_builtin_arguments(context, builtin_name, &encoded, metadata)?;
            execute_baseline_native_builtin_control(
                context,
                builtin_name,
                &expanded,
                instruction,
                Some((frame.function_id, local_values)),
                None,
            )
            })()
            .map_err(|control| match control {
                NativeCallControl::RuntimeError(message)
                    if message.starts_with("native runtime value ") =>
                {
                    NativeCallControl::RuntimeError(format!(
                        "{message} while executing {:?}",
                        instruction.kind
                    ))
                }
                control => control,
            });
                if DIAGNOSTIC {
                    let inclusive_nanos = callsite_started_at
                        .map(|started_at| {
                            started_at.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
                        })
                        .unwrap_or(0);
                    if direct_builtin && let Some(entry) = descriptor.direct_builtin {
                        let mut telemetry = context.runtime_telemetry.borrow_mut();
                        let elapsed = telemetry
                            .counters
                            .native_builtin_time_nanos_by_name
                            .entry(entry.entry.name().to_owned())
                            .or_default();
                        *elapsed = elapsed.saturating_add(inclusive_nanos);
                    }
                    context.record_native_callsite_timing(
                        frame.function_id,
                        frame.source_block_id,
                        frame.source_instruction_id,
                        inclusive_nanos,
                        context.active_helper_child_time_nanos(),
                    );
                    context.exit_runtime_helper(helper_id);
                }
                if let std::borrow::Cow::Owned(mut encoded) = encoded {
                    encoded.clear();
                    context.native_call_encoded_scratch = encoded;
                }
                outcome
            },
        );
        match outcome {
            Some(Ok(value)) => {
                let status = if frame.flags & (1 << 1) != 0 {
                    php_jit::JitCallStatus::RETURN_REFERENCE
                } else {
                    php_jit::JitCallStatus::RETURN
                };
                (status.0 as i32, status, Some(value))
            }
            Some(Err(NativeCallControl::Rethrow)) => {
                let source_span = callsite_span;
                let value = with_baseline_native_context_for(runtime, "call_dispatch", |context| {
                    let mut throwable = context.take_pending_throwable()?;
                    if let Some(source_span) = source_span {
                        throwable =
                            native_throwable_with_call_source(context, throwable, source_span);
                    }
                    context.encode_baseline_value(throwable).ok()
                })
                .flatten();
                (
                    php_jit::JitCallStatus::THROW.0 as i32,
                    php_jit::JitCallStatus::THROW,
                    value,
                )
            }
            Some(Err(NativeCallControl::Throw { class, message })) => {
                let source_span = callsite_span;
                let value = with_baseline_native_context_for(runtime, "call_dispatch", |context| {
                    let target = (!compact_arguments
                        && frame.flags & php_jit::JitNativeCallFrame::FLAG_DIRECT_BUILTIN == 0
                        && frame.target.function_id != u32::MAX)
                        .then(|| php_ir::FunctionId::new(frame.target.function_id))
                        .and_then(|function| context.unit.functions.get(function.index()))
                        .map(|function| (function.span, function.name.clone()));
                    if class.eq_ignore_ascii_case("TypeError")
                        && message.contains("Argument #")
                        && let Some((target_span, target_name)) = target
                    {
                        let encoded =
                            encode_native_throwable_at(context, &class, &message, target_span)
                                .ok()?;
                        let throwable = context.decode_baseline_value(encoded).ok()?;
                        let arguments = arguments
                            .iter()
                            .map(|argument| {
                                context.decode_baseline_value(argument.value.payload as i64)
                            })
                            .collect::<Result<Vec<_>, _>>()
                            .ok()?;
                        let mut throwable =
                            native_throwable_with_frame(throwable, &target_name, arguments);
                        if let Some(source_span) = source_span {
                            throwable =
                                native_throwable_with_call_source(context, throwable, source_span);
                        }
                        return context.encode_baseline_value(throwable).ok();
                    }
                    source_span
                        .and_then(|span| {
                            encode_native_throwable_at(context, &class, &message, span).ok()
                        })
                        .or_else(|| encode_native_throwable(context, &class, &message).ok())
                })
                .flatten();
                (
                    php_jit::JitCallStatus::THROW.0 as i32,
                    php_jit::JitCallStatus::THROW,
                    value,
                )
            }
            Some(Err(NativeCallControl::ArgumentCount {
                function,
                passed,
                required,
                target_span,
            })) => {
                let source_span = callsite_span.unwrap_or(target_span);
                let value = with_baseline_native_context_for(runtime, "call_dispatch", |context| {
                    let message = native_argument_count_message(
                        context,
                        &function,
                        passed,
                        required,
                        source_span,
                    );
                    let encoded = encode_native_throwable_at(
                        context,
                        "ArgumentCountError",
                        &message,
                        target_span,
                    )
                    .ok()?;
                    let throwable = context.decode_baseline_value(encoded).ok()?;
                    let decoded_arguments = arguments
                        .iter()
                        .map(|argument| {
                            context.decode_baseline_value(argument.value.payload as i64)
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .ok()?;
                    let throwable =
                        native_throwable_with_frame(throwable, &function, decoded_arguments);
                    let throwable =
                        native_throwable_with_call_source(context, throwable, source_span);
                    context.encode_baseline_value(throwable).ok()
                })
                .flatten();
                (
                    php_jit::JitCallStatus::THROW.0 as i32,
                    php_jit::JitCallStatus::THROW,
                    value,
                )
            }
            Some(Err(NativeCallControl::Propagate { status, value })) => {
                (status.0 as i32, status, Some(value))
            }
            Some(Err(NativeCallControl::SuspendFiber { state })) => {
                if let Some(state) = state
                    && frame.transition_state != 0
                {
                    // SAFETY: generated call frames publish their live
                    // caller-owned `JitDeoptState` buffer for this synchronous
                    // dispatch. The caller consumes it before returning.
                    unsafe {
                        (frame.transition_state as usize as *mut php_jit::JitDeoptState)
                            .write(*state);
                    }
                }
                let value = with_baseline_native_context_for(runtime, "call_dispatch", |context| {
                    context.pending_fiber_suspension_value.take()
                })
                .flatten();
                (
                    php_jit::JitCallStatus::SUSPEND_FIBER.0 as i32,
                    php_jit::JitCallStatus::SUSPEND_FIBER,
                    value,
                )
            }
            Some(Err(NativeCallControl::Exit(value))) => (
                php_jit::JitCallStatus::EXIT.0 as i32,
                php_jit::JitCallStatus::EXIT,
                Some(value),
            ),
            Some(Err(NativeCallControl::PublishedRuntimeError)) => (
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                php_jit::JitCallStatus::RUNTIME_ERROR,
                None,
            ),
            Some(Err(NativeCallControl::RuntimeError(message))) => {
                let _ = with_baseline_native_context_for(runtime, "call_dispatch", |context| {
                    publish_native_call_diagnostic(context, message)
                });
                (
                    php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                    php_jit::JitCallStatus::RUNTIME_ERROR,
                    None,
                )
            }
            None => (
                php_jit::JitCallStatus::COMPILE_REQUIRED.0 as i32,
                php_jit::JitCallStatus::COMPILE_REQUIRED,
                None,
            ),
        }
    };
    let (status, call_status, value) = result;
    // SAFETY: `out` is a checked, caller-owned result record.
    unsafe {
        out.write(php_jit::JitCallResult {
            status: call_status,
            detail: status as u32,
            value: value.map_or_else(php_jit::JitAbiSlot::default, |value| php_jit::JitAbiSlot {
                tag: 3,
                flags: 0,
                payload: value as u64,
            }),
        });
    }
    status
}
