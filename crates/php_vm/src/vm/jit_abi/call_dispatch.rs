use super::*;

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
    context: &NativeExecutionContext<'_>,
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
                    .position(|parameter| parameter.name.eq_ignore_ascii_case(name))
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
                .position(|parameter| parameter.name.eq_ignore_ascii_case(name))
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
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_native_builtin_dispatch_abi(
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
    out: *mut php_jit::JitCallResult,
) -> i32 {
    // SAFETY: production publication validates the immutable callsite and
    // generated argument shape before this entry can execute.
    unsafe {
        jit_native_builtin_dispatch_impl::<false>(
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
            out,
        )
    }
}

// SAFETY: diagnostic publication uses the same trusted internal ABI and adds
// accounting in a separately compiled function.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_native_builtin_dispatch_diagnostic_abi(
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
    out: *mut php_jit::JitCallResult,
) -> i32 {
    // SAFETY: diagnostic publication validates the same generated ABI.
    unsafe {
        jit_native_builtin_dispatch_impl::<true>(
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
            out,
        )
    }
}

#[allow(unsafe_code)]
unsafe fn jit_native_builtin_dispatch_impl<const DIAGNOSTIC: bool>(
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
    let outcome = with_native_context_for(runtime, "call_dispatch", |context| {
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
        let result = if matches!(
            entry.execution_kind(),
            php_runtime::api::BuiltinExecutionKind::Runtime
        ) {
            execute_prepared_runtime_builtin(context, arguments, callsite_span, prepared)
        } else {
            let instruction = php_ir::Instruction {
                id: php_ir::InstrId::new(0),
                span: callsite_span,
                kind: php_ir::InstructionKind::Nop,
            };
            execute_native_builtin(
                context,
                entry.name(),
                arguments,
                &instruction,
                Some((function, local_slots)),
                Some(prepared),
            )
        };
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

    finish_native_dispatch_outcome(runtime, outcome, Some(callsite_span), out)
}

// Converts one trusted internal dispatch outcome into the stable native
// control result. PHP-visible throw/suspend/exit semantics remain centralized
// here while individual prepared dispatchers avoid the generic call frame.
// SAFETY: `out` is owned by generated code for the synchronous helper call.
#[allow(unsafe_code)]
pub(super) fn finish_native_dispatch_outcome(
    runtime: *mut NativeRequestFastState,
    outcome: Option<Result<i64, String>>,
    callsite_span: Option<php_ir::IrSpan>,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    debug_assert!(!out.is_null());
    let (status, value) = match outcome {
        Some(Ok(value)) => (php_jit::JitCallStatus::RETURN, Some(value)),
        Some(Err(message)) if message == "E_PHP_RETHROW" => {
            let value = with_native_context_for(runtime, "call_dispatch", |context| {
                let mut throwable = context.take_pending_throwable()?;
                if let Some(span) = callsite_span {
                    throwable = native_throwable_with_call_source(context, throwable, span);
                }
                context.encode(throwable).ok()
            })
            .flatten();
            (php_jit::JitCallStatus::THROW, value)
        }
        Some(Err(message)) if message.starts_with("E_PHP_THROW:") => {
            let payload = message.trim_start_matches("E_PHP_THROW:");
            let (class, message) = payload.split_once(':').unwrap_or(("Error", payload));
            let value = with_native_context_for(runtime, "call_dispatch", |context| {
                callsite_span
                    .and_then(|span| encode_native_throwable_at(context, class, message, span).ok())
                    .or_else(|| encode_native_throwable(context, class, message).ok())
            })
            .flatten();
            (php_jit::JitCallStatus::THROW, value)
        }
        Some(Err(message)) if message == "E_PHP_SUSPEND_FIBER" => {
            let value = with_native_context_for(runtime, "call_dispatch", |context| {
                context.pending_fiber_suspension_value.take()
            })
            .flatten();
            (php_jit::JitCallStatus::SUSPEND_FIBER, value)
        }
        Some(Err(message)) if message.starts_with("E_PHP_EXIT:") => (
            php_jit::JitCallStatus::EXIT,
            message
                .trim_start_matches("E_PHP_EXIT:")
                .parse::<i64>()
                .ok(),
        ),
        Some(Err(message)) if message == NATIVE_RUNTIME_ERROR_MARKER => {
            (php_jit::JitCallStatus::RUNTIME_ERROR, None)
        }
        Some(Err(message)) => {
            let _ = with_native_context_for(runtime, "call_dispatch", |context| {
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
pub(in crate::vm) extern "C" fn jit_native_call_dispatch_abi(
    runtime: *mut NativeRequestFastState,
    vm_context: u64,
    frame: *mut php_jit::JitNativeCallFrame,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    // SAFETY: production publication validates the generated frame ABI.
    unsafe { jit_native_call_dispatch_impl::<false>(runtime, vm_context, frame, out) }
}

// SAFETY: diagnostic publication uses the same generated ABI and adds
// accounting in a separately compiled entry.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_native_call_dispatch_diagnostic_abi(
    runtime: *mut NativeRequestFastState,
    vm_context: u64,
    frame: *mut php_jit::JitNativeCallFrame,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    // SAFETY: diagnostic publication validates the same frame contract.
    unsafe { jit_native_call_dispatch_impl::<true>(runtime, vm_context, frame, out) }
}

#[allow(unsafe_code)]
unsafe fn jit_native_call_dispatch_impl<const DIAGNOSTIC: bool>(
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
    let result = (|| {
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
        let outcome = with_native_context_for(runtime, "call_dispatch", |context| {
            let descriptor =
                context.prepared_native_callsite(frame.function_id, frame.continuation_id);
            let Some(descriptor) = descriptor else {
                return Err(format!(
                    "E_PHP_VM_UNRESOLVED_CALLABLE: native call site is unavailable at function={} block={} instruction={}",
                    frame.function_id, frame.source_block_id, frame.source_instruction_id,
                ));
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
            let (mut encoded, encoded_capacity_before) = if compact_arguments && direct_builtin {
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
                    let dynamic_target = descriptor.target_symbol.as_deref().unwrap_or_else(|| {
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
            let outcome = (|| {
            let completed_nested_fiber_matches = context
                .completed_nested_fiber_call
                .as_ref()
                .is_some_and(|(function, continuation, _)| {
                    *function == frame.function_id && *continuation == frame.continuation_id
                });
            if completed_nested_fiber_matches
                && let Some((_, _, value)) = context.completed_nested_fiber_call.take()
            {
                return Ok(value);
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
                materialize_native_property_reference_arguments(
                    context,
                    arguments,
                    encoded.to_mut(),
                    metadata,
                )?;
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
                return execute_native_builtin(
                    context,
                    entry.entry.name(),
                    &expanded,
                    instruction,
                    Some((frame.function_id, local_values)),
                    Some(entry),
                );
            }
            if let Some(operation) = semantic_operation {
                return execute_native_semantic_operation(
                    context,
                    operation,
                    instruction,
                    &encoded,
                    frame.function_id,
                );
            }
            if let Some(result) =
                execute_native_static_property(context, instruction, &encoded, frame.function_id)
            {
                return result;
            }
            if let Some(result) = execute_native_fiber_suspend(context, instruction, &encoded) {
                return result;
            }
            if let Some(result) = execute_native_instanceof(context, instruction, &encoded) {
                return result;
            }
            if let Some(result) = execute_native_resolve_callable(context, instruction) {
                return result;
            }
            if let Some(result) = execute_native_acquire_callable(context, instruction, &encoded) {
                return result;
            }
            if let Some(result) = execute_native_bind_global(context, instruction) {
                return result;
            }
            if matches!(
                instruction.kind,
                php_ir::InstructionKind::CallCallable { .. }
                    | php_ir::InstructionKind::CallClosure { .. }
                    | php_ir::InstructionKind::Pipe { .. }
            ) && !direct_builtin
                && frame.target.function_id != u32::MAX
            {
                let function = php_ir::FunctionId::new(frame.target.function_id);
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
                if native_function_is_generator(context, function) {
                    let arguments = invocation_arguments
                        .iter()
                        .map(|value| context.decode(*value))
                        .collect::<Result<Vec<_>, _>>()?;
                    return context.encode(Value::Generator(php_runtime::api::GeneratorRef::new(
                        function.raw(),
                        arguments,
                    )));
                }
                let metadata = match instruction.kind {
                    php_ir::InstructionKind::CallCallable { .. }
                    | php_ir::InstructionKind::CallClosure { .. } => {
                        Some(descriptor.arguments.as_ref())
                    }
                    php_ir::InstructionKind::Pipe { .. } => None,
                    _ => None,
                };
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
                return result;
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
            ) {
                return result;
            }
            if let Some(result) =
                execute_native_class_constant(context, instruction, frame.function_id)
            {
                return result;
            }
            if let Some(result) = execute_native_internal_class(context, instruction, &encoded) {
                return result;
            }
            if let Some(result) = execute_native_array_object(context, instruction, &encoded) {
                return result;
            }
            if let Some(result) = execute_native_enum_static_method(context, instruction, &encoded)
            {
                return result;
            }
            if let Some(result) = execute_native_generator_method(context, instruction, &encoded) {
                return result;
            }
            if let Some(result) = execute_native_fiber_method(context, instruction, &encoded) {
                return result;
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
                let callback = context.decode(callback)?;
                if !matches!(callback, Value::Callable(_)) {
                    return Err(
                        "Fiber::__construct(): Argument #1 ($callback) must be of type callable"
                            .to_owned(),
                    );
                }
                return context.encode(Value::Fiber(php_runtime::api::FiberRef::new(callback)));
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
                            .map(|message| context.decode(*message))
                            .transpose()?
                            .map(native_string)
                            .transpose()?
                            .map_or_else(String::new, |message| {
                                String::from_utf8_lossy(&message).into_owned()
                            });
                        return encode_native_throwable_at(
                            context,
                            &class.display_name,
                            &message,
                            instruction.span,
                        );
                    }
                    native_prepare_runtime_class_constants(
                        context,
                        None,
                        &class,
                        instruction,
                    )?;
                    let object = new_native_object(context, None, &class)?;
                    return context.encode(Value::Object(object));
                }
                if !native_external_class_exists(context, &display_class_name)
                    && context.autoload_in_progress.insert(normalized.clone())
                {
                    let callbacks = context.autoload_callbacks.clone();
                    for callback in callbacks {
                        invoke_native_callable_value(
                            context,
                            callback,
                            &[Value::String(PhpString::from_bytes(
                                display_class_name.as_bytes().to_vec(),
                            ))],
                            instruction,
                            None,
                        )?;
                        if native_external_class_exists(context, &display_class_name) {
                            break;
                        }
                    }
                    context.autoload_in_progress.remove(&normalized);
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
                    return create_native_external_object(
                        context,
                        &display_class_name,
                        &encoded,
                        instruction,
                    );
                }
            }
            if let php_ir::InstructionKind::CallMethod { method, .. } = &instruction.kind
                && (method.eq_ignore_ascii_case("getMessage")
                    || method.eq_ignore_ascii_case("getTrace")
                    || method.eq_ignore_ascii_case("getCode")
                    || method.eq_ignore_ascii_case("getPrevious"))
                && let Some(receiver) = encoded.first()
            {
                let decoded = context.decode(*receiver)?;
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
                        return Err(format!(
                            "Call to a member function {method}() on {}",
                            native_value_type_name(&value)
                        ));
                    }
                };
                return context.encode(value);
            }
            if let php_ir::InstructionKind::CallMethod { method, args, .. } = &instruction.kind
                && let Some(receiver) = encoded.first()
            {
                let receiver_value = match context.decode(*receiver).map_err(|error| {
                    format!("{method}() native receiver could not be decoded: {error}")
                })? {
                    Value::Reference(reference) => reference.get(),
                    value => value,
                };
                let Value::Object(object) = receiver_value else {
                    return Err(format!(
                        "Call to a member function {method}() on {}",
                        native_value_type_name(&receiver_value)
                    ));
                };
                let class_name = object.class_name();
                if let Some(target) =
                    context.lookup_native_method_pic(descriptor, &class_name, method)
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
                            let method_name = context.encode(Value::String(
                                PhpString::from_bytes(method.as_bytes().to_vec()),
                            ))?;
                            let call_arguments =
                                encode_native_call_arguments_array(context, &encoded[1..])?;
                            return invoke_native_function(
                                context,
                                magic,
                                &[*receiver, method_name, call_arguments],
                            );
                        }
                        return Err(format!("E_PHP_THROW:Error:{error}"));
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
                        let arguments = call_arguments
                            .iter()
                            .map(|value| context.decode(*value))
                            .collect::<Result<Vec<_>, _>>()?;
                        return context.encode(Value::Generator(
                            php_runtime::api::GeneratorRef::new(function.raw(), arguments),
                        ));
                    }
                    if context.install_native_method_pic(
                        descriptor,
                        &class_name,
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
                        return Err(format!("E_PHP_THROW:Error:{error}"));
                    }
                    let call_arguments = if entry.flags.is_static {
                        &encoded[1..]
                    } else {
                        &encoded
                    };
                    if context.install_native_method_pic(
                        descriptor,
                        &class_name,
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
                    let method_name = context.encode(Value::String(PhpString::from_bytes(
                        method.as_bytes().to_vec(),
                    )))?;
                    let call_arguments =
                        encode_native_call_arguments_array(context, &encoded[1..])?;
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
                        return Err(format!("E_PHP_THROW:Error:{error}"));
                    }
                    let method_name = context.encode(Value::String(PhpString::from_bytes(
                        method.as_bytes().to_vec(),
                    )))?;
                    let call_arguments =
                        encode_native_call_arguments_array(context, &encoded[1..])?;
                    return invoke_native_external_function(
                        context,
                        function,
                        &[*receiver, method_name, call_arguments],
                        Some(class_name),
                        context.unit.strict_types_for_span(instruction.span),
                    );
                }
            }
            if let php_ir::InstructionKind::CallStaticMethod {
                class_name,
                method,
                ..
            } = &instruction.kind
            {
                if class_name.eq_ignore_ascii_case("Closure") && method.eq_ignore_ascii_case("bind")
                {
                    let closure = encoded
                        .first()
                        .copied()
                        .ok_or_else(|| "Closure::bind() expects a closure".to_owned())?;
                    let closure = context.decode(closure)?;
                    let Value::Callable(callable) = closure else {
                        return Err("Closure::bind() expects a closure".to_owned());
                    };
                    let php_runtime::api::CallableValue::Closure(closure) = callable.as_ref()
                    else {
                        return Err("Closure::bind() expects a closure".to_owned());
                    };
                    let rebound = native_rebind_closure(
                        closure,
                        encoded
                            .get(1)
                            .copied()
                            .map(|value| context.decode(value))
                            .transpose()?,
                        encoded
                            .get(2)
                            .copied()
                            .map(|value| context.decode(value))
                            .transpose()?,
                    )?;
                    return context.encode(rebound);
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
                    _ => Some(class_name.clone()),
                };
                if let Some(class) = resolved_class.as_deref() {
                    native_autoload_class(context, class, instruction)?;
                }
                if let Some(result) = resolved_class.as_deref().and_then(|class| {
                    initialize_native_throwable_parent(context, class, method, &encoded)
                }) {
                    return result;
                }
                if let Some(class) = resolved_class.as_deref()
                    && let Some(target) =
                        context.lookup_native_method_pic(descriptor, class, method)
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
                            let method_name = context.encode(Value::String(
                                PhpString::from_bytes(method.as_bytes().to_vec()),
                            ))?;
                            let call_arguments =
                                encode_native_call_arguments_array(context, &encoded)?;
                            return invoke_native_function(
                                context,
                                magic,
                                &[method_name, call_arguments],
                            );
                        }
                        return Err(format!("E_PHP_THROW:Error:{error}"));
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
                        ));
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
                        return Err(format!("E_PHP_THROW:Error:{error}"));
                    }
                    if !entry.flags.is_static && frame.receiver_handle == 0 {
                        return Err(format!(
                            "Non-static method {class}::{method}() cannot be called statically"
                        ));
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
                    let method_name = context.encode(Value::String(PhpString::from_bytes(
                        method.as_bytes().to_vec(),
                    )))?;
                    let call_arguments = encode_native_call_arguments_array(context, &encoded)?;
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
                            return Err(format!("E_PHP_THROW:Error:{error}"));
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
                    return Err(format!("E_PHP_THROW:Error:{error}"));
                }
                return invoke_native_function_with_metadata_strict(
                    context,
                    constructor,
                    &encoded,
                    Some(args),
                    context.unit.strict_types_for_span(instruction.span),
                );
            }
            let name = match &instruction.kind {
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
                    ));
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
                        ));
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
                    ));
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
                        .and_then(|receiver| context.decode(*receiver).ok())
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
                    ));
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
                    ));
                }
                php_ir::InstructionKind::BindReferenceFromMethodCall { method, .. } => {
                    return Err(format!(
                        "E_PHP_VM_UNKNOWN_METHOD: by-reference method call {method}() is not implemented"
                    ));
                }
                _ => None,
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
                ));
            };
            if matches!(
                instruction.kind,
                php_ir::InstructionKind::CallCallable { .. }
            ) && context.function_id(name.as_ref()).is_none()
            {
                return Err(format!(
                    "E_PHP_THROW:Error:Call to undefined function {name}()"
                ));
            }
            if !direct_builtin
                && let Some(function_id) = context.function_id(name.as_ref())
            {
                emit_native_deprecated_call(context, function_id, instruction);
                if matches!(
                    instruction.kind,
                    php_ir::InstructionKind::BindReferenceFromCall { .. }
                ) && context
                    .unit
                    .functions
                    .get(function_id.index())
                    .is_some_and(|function| !function.returns_by_ref)
                {
                    let path = context
                        .unit
                        .files
                        .get(instruction.span.file.index())
                        .map_or("<unknown>", |file| file.path.as_str());
                    let line = native_source_line(context, instruction);
                    context.output.write_bytes(format!(
                        "\nNotice: Only variables should be assigned by reference in {path} on line {line}\n"
                    ));
                }
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
                        ));
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
                    let visible_arguments = encoded
                        .iter()
                        .map(|value| context.decode(*value))
                        .collect::<Result<Vec<_>, _>>()?;
                    return context.encode(Value::Generator(php_runtime::api::GeneratorRef::new(
                        function_id.raw(),
                        visible_arguments,
                    )));
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
                materialize_native_property_reference_arguments(
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
                materialize_native_property_reference_arguments(
                    context,
                    arguments,
                    encoded.to_mut(),
                    metadata,
                )?;
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
                ));
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
            execute_native_builtin(
                context,
                builtin_name,
                &expanded,
                instruction,
                Some((frame.function_id, local_values)),
                None,
            )
            })()
            .map_err(|message| {
                if message.starts_with("native runtime value ") {
                    format!("{message} while executing {:?}", instruction.kind)
                } else {
                    message
                }
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
        });
        match outcome {
            Some(Ok(value)) => {
                let status = if frame.flags & (1 << 1) != 0 {
                    php_jit::JitCallStatus::RETURN_REFERENCE
                } else {
                    php_jit::JitCallStatus::RETURN
                };
                (status.0 as i32, status, Some(value))
            }
            Some(Err(message)) if message == "E_PHP_RETHROW" => {
                let source_span = callsite_span;
                let value = with_native_context_for(runtime, "call_dispatch", |context| {
                    let mut throwable = context.take_pending_throwable()?;
                    if let Some(source_span) = source_span {
                        throwable =
                            native_throwable_with_call_source(context, throwable, source_span);
                    }
                    context.encode(throwable).ok()
                })
                .flatten();
                (
                    php_jit::JitCallStatus::THROW.0 as i32,
                    php_jit::JitCallStatus::THROW,
                    value,
                )
            }
            Some(Err(message)) if message.starts_with("E_PHP_THROW:") => {
                let payload = message.trim_start_matches("E_PHP_THROW:");
                let (class, message) = payload.split_once(':').unwrap_or(("Error", payload));
                let source_span = callsite_span;
                let value = with_native_context_for(runtime, "call_dispatch", |context| {
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
                            encode_native_throwable_at(context, class, message, target_span)
                                .ok()?;
                        let throwable = context.decode(encoded).ok()?;
                        let arguments = arguments
                            .iter()
                            .map(|argument| context.decode(argument.value.payload as i64))
                            .collect::<Result<Vec<_>, _>>()
                            .ok()?;
                        let mut throwable =
                            native_throwable_with_frame(throwable, &target_name, arguments);
                        if let Some(source_span) = source_span {
                            throwable =
                                native_throwable_with_call_source(context, throwable, source_span);
                        }
                        return context.encode(throwable).ok();
                    }
                    source_span
                        .and_then(|span| {
                            encode_native_throwable_at(context, class, message, span).ok()
                        })
                        .or_else(|| encode_native_throwable(context, class, message).ok())
                })
                .flatten();
                (
                    php_jit::JitCallStatus::THROW.0 as i32,
                    php_jit::JitCallStatus::THROW,
                    value,
                )
            }
            Some(Err(message)) if message == "E_PHP_SUSPEND_FIBER" => {
                let value = with_native_context_for(runtime, "call_dispatch", |context| {
                    context.pending_fiber_suspension_value.take()
                })
                .flatten();
                (
                    php_jit::JitCallStatus::SUSPEND_FIBER.0 as i32,
                    php_jit::JitCallStatus::SUSPEND_FIBER,
                    value,
                )
            }
            Some(Err(message)) if message.starts_with("E_PHP_EXIT:") => {
                let value = message
                    .trim_start_matches("E_PHP_EXIT:")
                    .parse::<i64>()
                    .ok();
                (
                    php_jit::JitCallStatus::EXIT.0 as i32,
                    php_jit::JitCallStatus::EXIT,
                    value,
                )
            }
            Some(Err(message)) if message == NATIVE_RUNTIME_ERROR_MARKER => (
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                php_jit::JitCallStatus::RUNTIME_ERROR,
                None,
            ),
            Some(Err(message)) => {
                let _ = with_native_context_for(runtime, "call_dispatch", |context| {
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
    })();
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
