//! Shared native entry execution boundaries.

use super::{NativeRequestOwner, Vm, VmResult, activate_native_context};
use crate::compiled_unit::CompiledUnit;
use php_runtime::api::OutputBuffer;
use std::sync::Arc;

impl Vm {
    /// Execute a validated, zero-arity persistent native artifact through the
    /// same request context and value decoder used by freshly compiled code.
    pub(super) fn execute_cached_entry(
        &self,
        unit: &CompiledUnit,
        loaded: Arc<super::native_compile_cache::LoadedNativeUnit>,
        entry: php_ir::FunctionId,
        output: OutputBuffer,
    ) -> VmResult {
        let native_entries = Arc::clone(loaded.native_entries());
        let Some(handle) = native_entries.get(&entry).cloned() else {
            return VmResult::compile_error(
                output,
                format!(
                    "E_NATIVE_CACHE_ENTRY: cached root {} is missing",
                    entry.raw()
                ),
            );
        };
        let mut context = NativeRequestOwner::new(
            unit,
            unit.cache_identity(),
            &self.options,
            &self.worker_state,
            output,
            native_entries,
        );
        context.attach_root_deployment_image(unit.clone());
        let native_execution_started_at =
            self.options.collect_counters.then(std::time::Instant::now);
        context.record_native_direct_calls(&handle);
        let guard = activate_native_context(&mut context);
        let runtime = context.native_runtime_ptr();
        let outcome = handle.invoke_i64_with_native_unwind_runtime(
            &[],
            php_jit::JIT_RUNTIME_ABI_HASH,
            runtime,
            |types, value| {
                let class = context
                    .decode_result(value)
                    .ok()
                    .and_then(super::native_exception_fields)
                    .map(|(class, _, _)| class);
                class.is_some_and(|class| {
                    types.iter().any(|type_| {
                        type_.eq_ignore_ascii_case(&class)
                            || type_.eq_ignore_ascii_case("Throwable")
                            || (type_.eq_ignore_ascii_case("Exception")
                                && class.ends_with("Exception"))
                            || (type_.eq_ignore_ascii_case("Error")
                                && (class == "Error" || class.ends_with("Error")))
                    })
                })
            },
        );
        let outcome = super::jit_abi::resume_native_optimizing_exit(&mut context, outcome);
        let (exception_handled, exception_handler_error) = match &outcome {
            Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
                if *status == php_jit::JitCallStatus::THROW.0 as i32 =>
            {
                match context.handle_uncaught_throwable(*value) {
                    Ok(handled) => (handled, None),
                    Err(error) => (false, Some(error)),
                }
            }
            _ => (false, None),
        };
        let mut shutdown_throwable = None;
        let shutdown_error = exception_handler_error.or_else(|| {
            context.run_shutdown_callbacks().err().and_then(|error| {
                if error == "E_PHP_RETHROW"
                    && let Some(throwable) = context.take_pending_throwable()
                {
                    shutdown_throwable = Some(throwable);
                    None
                } else {
                    Some(error)
                }
            })
        });
        context.output.flush_all_buffers();
        drop(guard);
        let publish_error = context.publish_include_globals().err();
        let native_execution_time_nanos = native_execution_started_at.map_or(0, |started_at| {
            started_at.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
        });
        let runtime_counters = self.options.collect_counters.then(|| {
            let mut counters = context.runtime_counters();
            counters.native_execution_entries = counters.native_execution_entries.saturating_add(1);
            counters.native_region_entries = counters.native_region_entries.saturating_add(1);
            counters.native_execution_time_nanos = native_execution_time_nanos;
            counters
        });

        let http_response = std::mem::take(&mut context.http_response);
        let upload_registry = std::mem::take(&mut context.upload_registry);
        let session = std::mem::take(&mut context.session);
        let process_exit_terminates_process = context.process_exit_terminates_process();
        let mut result = if let Some(throwable) = shutdown_throwable {
            super::native_uncaught_throwable_result(
                std::mem::take(&mut context.output),
                Some(throwable),
            )
        } else if let Some(error) = shutdown_error.or(publish_error) {
            VmResult::runtime_error(
                std::mem::take(&mut context.output),
                context.diagnostic.take(),
                format!("E_NATIVE_SHUTDOWN: {error}"),
            )
        } else if exception_handled {
            VmResult::success(
                std::mem::take(&mut context.output),
                Some(php_runtime::api::Value::Null),
            )
        } else {
            match outcome {
                Ok(php_jit::JitI64InvokeOutcome::Returned(encoded)) => {
                    match context.decode_result(encoded) {
                        Ok(value) => {
                            let mut result =
                                VmResult::success(std::mem::take(&mut context.output), Some(value));
                            result.diagnostics.extend(context.diagnostic.take());
                            result
                        }
                        Err(error) => VmResult::runtime_error(
                            std::mem::take(&mut context.output),
                            context.diagnostic.take(),
                            format!("E_NATIVE_VALUE: {error}"),
                        ),
                    }
                }
                Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
                    if status == php_jit::JitCallStatus::EXIT.0 as i32 =>
                {
                    let exit_code = match context.decode_result(value) {
                        Ok(php_runtime::api::Value::String(value)) => {
                            context.output.write_bytes(value.as_bytes());
                            0
                        }
                        Ok(php_runtime::api::Value::Int(value)) => {
                            i32::try_from(value).unwrap_or(0)
                        }
                        Ok(php_runtime::api::Value::Bool(value)) => i32::from(value),
                        _ => 0,
                    };
                    VmResult::success_exit(std::mem::take(&mut context.output), exit_code)
                }
                Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
                    if status == php_jit::JitCallStatus::THROW.0 as i32 =>
                {
                    let throwable = context.decode_result(value).ok();
                    super::native_uncaught_throwable_result(
                        std::mem::take(&mut context.output),
                        throwable,
                    )
                }
                Ok(php_jit::JitI64InvokeOutcome::SideExit { status, state, .. })
                    if status == php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32 =>
                {
                    let operation =
                        context.instruction_kind_debug(state.function_id, state.continuation_id);
                    let message = context
                        .diagnostic
                        .as_ref()
                        .map_or_else(
                            || {
                                format!(
                                    "native runtime operation failed at function {} continuation {} ({}) native version {} control {} marker {:#x} value {}",
                                    state.function_id,
                                    state.continuation_id,
                                    operation,
                                    state.native_version,
                                    state.control_status.0,
                                    state.control_reserved,
                                    state.control_value,
                                )
                            },
                            |diagnostic| diagnostic.message().to_owned(),
                        );
                    if context.diagnostic.as_ref().is_some_and(|diagnostic| {
                        diagnostic.severity() == php_runtime::api::RuntimeSeverity::FatalError
                    }) && context
                        .output
                        .as_bytes()
                        .windows(b"Fatal error".len())
                        .any(|window| window == b"Fatal error")
                    {
                        VmResult::fatal(
                            std::mem::take(&mut context.output),
                            context.diagnostic.take(),
                            message,
                        )
                    } else {
                        VmResult::runtime_error(
                            std::mem::take(&mut context.output),
                            context.diagnostic.take(),
                            message,
                        )
                    }
                }
                Ok(php_jit::JitI64InvokeOutcome::SideExit { status, .. })
                    if status == php_jit::JitCallStatus::RETURN_REFERENCE.0 as i32 =>
                {
                    VmResult::success(std::mem::take(&mut context.output), None)
                }
                Ok(php_jit::JitI64InvokeOutcome::SideExit { status, .. }) => {
                    VmResult::runtime_error(
                        std::mem::take(&mut context.output),
                        context.diagnostic.take(),
                        format!("cached native entry returned status {status}"),
                    )
                }
                Err(error) => VmResult::compile_error(
                    std::mem::take(&mut context.output),
                    format!("E_NATIVE_CACHE_ENTRY: cached entry invocation failed: {error:?}"),
                ),
            }
        };
        context.recycle_native_request_buffers();
        result.process_exit_terminates_process = process_exit_terminates_process;
        result.http_response = Some(Box::new(http_response));
        result.upload_registry = Some(Box::new(upload_registry));
        result.session = Some(Box::new(session));
        if let Some(runtime_counters) = runtime_counters {
            result.counters = Some(Box::new(runtime_counters));
        }
        result
    }
}
