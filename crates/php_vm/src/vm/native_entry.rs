//! Shared native entry execution boundaries.

use super::{NativeExecutionContext, Vm, VmResult, activate_native_context};
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
        let mut context = NativeExecutionContext::new(
            unit,
            unit.cache_identity(),
            &self.options,
            &self.worker_state,
            output,
            native_entries,
        );
        context.install_root_dynamic_unit(unit.clone());
        let native_execution_started_at =
            self.options.collect_counters.then(std::time::Instant::now);
        context.record_native_direct_calls(&handle);
        let guard = activate_native_context(&mut context);
        let invocation = handle.invoke_i64_with_native_unwind(
            &[],
            php_jit::JIT_RUNTIME_ABI_HASH,
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
        drop(guard);
        context.publish_include_globals();
        let native_execution_time_nanos = native_execution_started_at.map_or(0, |started_at| {
            started_at.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
        });
        let mut runtime_counters = context.runtime_counters();
        runtime_counters.native_execution_entries =
            runtime_counters.native_execution_entries.saturating_add(1);
        runtime_counters.native_region_entries =
            runtime_counters.native_region_entries.saturating_add(1);
        runtime_counters.native_execution_time_nanos = native_execution_time_nanos;

        let mut result = match invocation {
            Ok(php_jit::JitI64InvokeOutcome::Returned(encoded)) => {
                match context.decode_result(encoded) {
                    Ok(value) => VmResult::success(context.output, Some(value)),
                    Err(error) => VmResult::runtime_error(
                        context.output,
                        context.diagnostic,
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
                    Ok(php_runtime::api::Value::Int(value)) => i32::try_from(value).unwrap_or(0),
                    Ok(php_runtime::api::Value::Bool(value)) => i32::from(value),
                    _ => 0,
                };
                VmResult::success_exit(context.output, exit_code)
            }
            Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
                if status == php_jit::JitCallStatus::THROW.0 as i32 =>
            {
                let throwable = context.decode_result(value).ok();
                super::native_uncaught_throwable_result(context.output, throwable)
            }
            Ok(php_jit::JitI64InvokeOutcome::SideExit { status, .. }) => VmResult::runtime_error(
                context.output,
                context.diagnostic,
                format!("cached native entry returned status {status}"),
            ),
            Err(error) => VmResult::compile_error(
                context.output,
                format!("E_NATIVE_CACHE_ENTRY: cached entry invocation failed: {error:?}"),
            ),
        };
        if self.options.collect_counters {
            result.counters = Some(Box::new(runtime_counters));
        }
        result
    }
}
