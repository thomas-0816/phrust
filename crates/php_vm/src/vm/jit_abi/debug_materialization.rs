//! Exact debug/reflection materialization over the generated PHP shadow stack.
//!
//! Generated code owns frame push/pop and argument lifetimes. This cold leaf
//! only turns immutable metadata plus authoritative native encodings into the
//! PHP array requested by `debug_backtrace()`; it never resolves or invokes a
//! PHP body.

use super::*;

fn release_entries(
    context: &mut NativeRequestColdState<'_>,
    entries: &[php_jit::JitNativeDirectArrayEntry],
) {
    for entry in entries.iter().rev() {
        let _ = context.release(entry.value);
        let _ = context.release(entry.key);
    }
}

fn push_named(
    context: &mut NativeRequestColdState<'_>,
    entries: &mut Vec<php_jit::JitNativeDirectArrayEntry>,
    name: &[u8],
    value: i64,
) -> Result<(), String> {
    let key = context
        .encode_native_string_bytes_owner(name)
        .map_err(|error| {
            let _ = context.release(value);
            error
        })?;
    entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
    Ok(())
}

fn trace_arguments(
    context: &NativeRequestColdState<'_>,
    frame: php_jit::JitNativeTraceFrame,
) -> Result<Vec<i64>, String> {
    let count = usize::try_from(frame.argument_count)
        .map_err(|_| "debug_backtrace() argument count overflow".to_owned())?;
    let fixed_count = usize::try_from(frame.fixed_argument_count)
        .unwrap_or(usize::MAX)
        .min(count);
    let fixed = if frame.fixed_arguments != 0 {
        frame.fixed_arguments
    } else {
        frame.arguments
    } as usize as *const i64;
    if fixed_count != 0 && fixed.is_null() {
        return Err("debug_backtrace() fixed argument storage is unavailable".to_owned());
    }
    let mut arguments = Vec::with_capacity(count);
    if fixed_count != 0 {
        // SAFETY: each generated shadow frame borrows this live caller stack
        // range until the corresponding generated call returns and pops it.
        #[allow(unsafe_code)]
        arguments.extend_from_slice(unsafe { std::slice::from_raw_parts(fixed, fixed_count) });
    }
    if frame.tail_arguments != 0 {
        let index = NativeRequestColdState::direct_value_index(frame.tail_arguments)
            .ok_or_else(|| "debug_backtrace() tail is not a direct native array".to_owned())?;
        let slot = context
            .direct_value_slots
            .get(index)
            .filter(|slot| {
                slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
            })
            .ok_or_else(|| "debug_backtrace() tail array is unavailable".to_owned())?;
        let length = usize::try_from(slot.payload).unwrap_or(usize::MAX);
        let entries = slot.aux as usize as *const php_jit::JitNativeDirectArrayEntry;
        if length != 0 && entries.is_null() {
            return Err("debug_backtrace() tail entries are unavailable".to_owned());
        }
        // SAFETY: the live tail owner keeps the direct entry range stable for
        // the generated call activation represented by this shadow frame.
        #[allow(unsafe_code)]
        arguments.extend(
            unsafe { std::slice::from_raw_parts(entries, length) }
                .iter()
                .map(|entry| entry.value),
        );
    } else if count > fixed_count {
        let all = frame.arguments as usize as *const i64;
        if all.is_null() {
            return Err("debug_backtrace() argument storage is unavailable".to_owned());
        }
        // SAFETY: same live generated caller range as the fixed prefix.
        #[allow(unsafe_code)]
        arguments
            .extend_from_slice(&unsafe { std::slice::from_raw_parts(all, count) }[fixed_count..]);
    }
    arguments.truncate(count);
    Ok(arguments)
}

fn encode_argument_list(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
) -> Result<i64, String> {
    let mut entries = Vec::with_capacity(arguments.len());
    for (index, argument) in arguments.iter().copied().enumerate() {
        let key = match context.encode_native_int(i64::try_from(index).unwrap_or(i64::MAX)) {
            Ok(key) => key,
            Err(error) => {
                release_entries(context, &entries);
                return Err(error);
            }
        };
        let value = match context.duplicate_authoritative_native_value(argument) {
            Ok(Some(value)) => Ok(value),
            Ok(None) => context.duplicate_baseline_call_argument(argument),
            Err(error) => Err(error),
        };
        let value = match value {
            Ok(value) => value,
            Err(error) => {
                let _ = context.release(key);
                release_entries(context, &entries);
                return Err(error);
            }
        };
        entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
    }
    context.publish_owned_direct_array_entries(entries)
}

fn encode_frame(
    context: &mut NativeRequestColdState<'_>,
    frame: php_jit::JitNativeTraceFrame,
    options: i64,
) -> Result<i64, String> {
    let metadata = (!frame.metadata.eq(&0)).then(|| {
        // SAFETY: publication stores an Arc-owned immutable metadata pointer;
        // the active compiled unit remains request-live while frames borrow it.
        #[allow(unsafe_code)]
        unsafe {
            &*(frame.metadata as usize
                as *const crate::compiled_unit::PreparedNativeFunctionMetadata)
        }
    });
    let mut entries = Vec::with_capacity(7);
    let result = (|| {
        if let Some(file) = metadata.and_then(|metadata| metadata.trace_file.as_ref()) {
            let value = context.encode_native_string_bytes_owner(file.as_bytes())?;
            push_named(context, &mut entries, b"file", value)?;
        }
        let line = metadata.map_or(0, |metadata| metadata.trace_line);
        if line > 0 {
            let value = context.encode_native_int(line)?;
            push_named(context, &mut entries, b"line", value)?;
        }
        let function = metadata.map_or(b"{unknown}".as_slice(), |metadata| {
            metadata.trace_function.as_bytes()
        });
        let value = context.encode_native_string_bytes_owner(function)?;
        push_named(context, &mut entries, b"function", value)?;
        if let Some(class) = metadata.and_then(|metadata| metadata.trace_class.as_ref()) {
            let value = context.encode_native_string_bytes_owner(class.as_bytes())?;
            push_named(context, &mut entries, b"class", value)?;
        }
        if let Some(call_type) = metadata.and_then(|metadata| metadata.trace_call_type) {
            let value = context.encode_native_string_bytes_owner(call_type.as_bytes())?;
            push_named(context, &mut entries, b"type", value)?;
        }
        if options & 1 != 0
            && frame.flags & php_jit::JIT_NATIVE_TRACE_HAS_RECEIVER != 0
            && frame.receiver != 0
        {
            let value = context
                .duplicate_authoritative_native_value(frame.receiver)?
                .ok_or_else(|| "debug_backtrace() receiver is unavailable".to_owned())?;
            push_named(context, &mut entries, b"object", value)?;
        }
        if options & 2 == 0 {
            let arguments = trace_arguments(context, frame)?;
            let value = encode_argument_list(context, &arguments)?;
            push_named(context, &mut entries, b"args", value)?;
        }
        Ok::<(), String>(())
    })();
    if let Err(error) = result {
        release_entries(context, &entries);
        return Err(error);
    }
    context.publish_owned_direct_array_entries(entries)
}

fn builtin_int_argument(
    context: &mut NativeRequestColdState<'_>,
    encoded: i64,
    strict: bool,
) -> Result<Option<i64>, String> {
    if php_jit::jit_decode_constant(encoded) == Some(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        return Ok(None);
    }
    let Some(checked) =
        context.coerce_native_call_argument_encoded(encoded, &php_ir::IrReturnType::Int, strict)?
    else {
        return Err("debug_backtrace() argument must be of type int".to_owned());
    };
    let value = context.native_encoded_int(checked);
    context.release(checked)?;
    value
        .map(Some)
        .ok_or_else(|| "debug_backtrace() integer coercion produced no integer".to_owned())
}

pub(in crate::vm) extern "C" fn jit_native_debug_backtrace_abi(
    runtime: *mut NativeRequestFastState,
    options: i64,
    limit: i64,
    strict: i64,
) -> php_jit::JitNativeControlResult {
    let result = with_baseline_native_context_for(runtime, "debug_backtrace", |context| {
        let options = builtin_int_argument(context, options, strict != 0)?.unwrap_or(1);
        let limit = builtin_int_argument(context, limit, strict != 0)?.unwrap_or(0);
        if limit < 0 {
            return Err(
                "debug_backtrace(): argument #2 ($limit) must be greater than or equal to 0"
                    .to_owned(),
            );
        }
        // SAFETY: activation publishes a fixed-capacity request-owned frame
        // arena and a depth no greater than that capacity.
        #[allow(unsafe_code)]
        let fast = unsafe { &*runtime };
        let depth = if fast.header.trace_depth == 0 {
            0
        } else {
            // SAFETY: `trace_depth` addresses the request-owned boxed counter.
            #[allow(unsafe_code)]
            unsafe {
                *(fast.header.trace_depth as usize as *const u32) as usize
            }
        }
        .min(fast.header.trace_capacity as usize);
        let frames = fast.header.trace_frames as usize as *const php_jit::JitNativeTraceFrame;
        if depth != 0 && frames.is_null() {
            return Err("debug_backtrace() shadow stack is unavailable".to_owned());
        }
        let take = if limit == 0 {
            depth
        } else {
            depth.min(usize::try_from(limit).unwrap_or(usize::MAX))
        };
        let mut entries = Vec::with_capacity(take);
        for output_index in 0..take {
            // SAFETY: output_index is bounded by the published depth/capacity.
            #[allow(unsafe_code)]
            let frame = unsafe { *frames.add(depth - output_index - 1) };
            let key =
                match context.encode_native_int(i64::try_from(output_index).unwrap_or(i64::MAX)) {
                    Ok(key) => key,
                    Err(error) => {
                        release_entries(context, &entries);
                        return Err(error);
                    }
                };
            let value = match encode_frame(context, frame, options) {
                Ok(value) => value,
                Err(error) => {
                    let _ = context.release(key);
                    release_entries(context, &entries);
                    return Err(error);
                }
            };
            entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
        }
        context.publish_owned_direct_array_entries(entries)
    });
    match result {
        Some(Ok(value)) => php_jit::JitNativeControlResult::returning(value),
        Some(Err(error)) => {
            // This debug leaf is allowed to publish a cold diagnostic, but it
            // never interprets or invokes PHP while doing so.
            #[allow(unsafe_code)]
            let context = unsafe { active_baseline_cold_context() };
            cold_diagnostics::record_native_helper_failure(context, error);
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
        None => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}
