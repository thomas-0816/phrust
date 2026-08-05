//! Diagnostic-only native helper entrypoints.

use super::*;

macro_rules! diagnostic_helper {
    ($wrapper:ident => $target:ident, $helper:expr, ($($name:ident: $ty:ty),* $(,)?) -> value_out) => {
        #[allow(unsafe_code)]
        pub(in crate::vm) extern "C" fn $wrapper(
            runtime: *mut NativeRequestFastState,
            $($name: $ty,)*
            out: *mut i64,
        ) -> i32 {
            debug_assert!(!runtime.is_null());
            // SAFETY: diagnostic baseline helpers execute synchronously and
            // use the same caller-owned output slot as the production ABI.
            unsafe { active_baseline_cold_context().enter_runtime_helper($helper) };
            let result = super::$target(runtime, $($name,)* out);
            if result == php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32 {
                // SAFETY: diagnostic wrappers receive a live request pointer
                // and run synchronously around the production baseline helper.
                let context = unsafe { active_baseline_cold_context() };
                if context.diagnostic.is_none() {
                    record_native_helper_failure(
                        context,
                        format!("diagnostic helper {} returned an unexplained runtime error", $helper),
                    );
                }
            }
            // SAFETY: the target returned before the request can be destroyed.
            unsafe { active_baseline_cold_context().exit_runtime_helper($helper) };
            result
        }
    };
    ($wrapper:ident => $target:ident, $helper:expr, ($($name:ident: $ty:ty),* $(,)?) -> $ret:ty) => {
        #[allow(unsafe_code)]
        pub(in crate::vm) extern "C" fn $wrapper(
            runtime: *mut NativeRequestFastState,
            $($name: $ty),*
        ) -> $ret {
            debug_assert!(!runtime.is_null());
            // SAFETY: diagnostic helpers receive the same live request pointer
            // as their production target and execute synchronously.
            unsafe { active_baseline_cold_context().enter_runtime_helper($helper) };
            let result = super::$target(runtime, $($name),*);
            // SAFETY: the target returned before the request can be destroyed.
            unsafe { active_baseline_cold_context().exit_runtime_helper($helper) };
            result
        }
    };
}

diagnostic_helper!(
    jit_native_function_resolve_diagnostic_abi => jit_native_function_resolve_abi,
    "function_resolve",
    (_vm_context: u64, function: u64) -> i32
);

diagnostic_helper!(
    jit_native_frame_alloc_diagnostic_abi => jit_native_frame_alloc_abi,
    "frame_arena",
    (_vm_context: u64, bytes: u64, alignment: u64) -> u64
);

diagnostic_helper!(
    jit_native_frame_release_diagnostic_abi => jit_native_frame_release_abi,
    "frame_arena",
    (_vm_context: u64, address: u64) -> i32
);

diagnostic_helper!(
    jit_cold_dynamic_unit_resolve_diagnostic_abi => jit_cold_dynamic_unit_resolve_abi,
    "dynamic_code",
    (_vm_context: u64, request: *mut php_jit::JitNativeDynamicCodeRequest, out: *mut php_jit::JitNativeDynamicUnitResolution) -> i32
);

diagnostic_helper!(
    jit_native_execution_poll_diagnostic_abi => jit_native_execution_poll_abi,
    "execution_poll",
    () -> i32
);

diagnostic_helper!(
    jit_native_declared_return_contract_diagnostic_abi => jit_native_declared_return_contract_abi,
    "declared_return_contract",
    (encoded: i64, contract: u64) -> value_out
);

diagnostic_helper!(
    jit_native_declared_argument_contract_diagnostic_abi => jit_native_declared_argument_contract_abi,
    "declared_argument_contract",
    (encoded: i64, contract: u64, strict: i64) -> value_out
);
