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
            unsafe { native_cold_context(runtime).enter_runtime_helper($helper) };
            let result = super::$target(runtime, $($name,)* out);
            if result == php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32 {
                // SAFETY: diagnostic wrappers receive a live request pointer
                // and run synchronously around the production baseline helper.
                let context = unsafe { native_cold_context(runtime) };
                if context.diagnostic.is_none() {
                    record_native_helper_failure(
                        context,
                        format!("diagnostic helper {} returned an unexplained runtime error", $helper),
                    );
                }
            }
            // SAFETY: the target returned before the request can be destroyed.
            unsafe { native_cold_context(runtime).exit_runtime_helper($helper) };
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
            unsafe { native_cold_context(runtime).enter_runtime_helper($helper) };
            let result = super::$target(runtime, $($name),*);
            // SAFETY: the target returned before the request can be destroyed.
            unsafe { native_cold_context(runtime).exit_runtime_helper($helper) };
            result
        }
    };
}

diagnostic_helper!(
    jit_native_function_resolve_diagnostic_abi => jit_native_function_resolve_abi,
    "function_resolve",
    (_vm_context: u64, function: u64, out: *mut usize) -> i32
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
    jit_native_dynamic_code_diagnostic_abi => jit_native_dynamic_code_abi,
    "dynamic_code",
    (_vm_context: u64, request: *mut php_jit::JitNativeDynamicCodeRequest, out: *mut php_jit::JitCallResult) -> i32
);

diagnostic_helper!(
    jit_native_execution_poll_diagnostic_abi => jit_native_execution_poll_abi,
    "execution_poll",
    () -> i32
);

diagnostic_helper!(
    jit_native_unary_diagnostic_abi => jit_native_unary_abi,
    "unary",
    (op: u32, src: i64) -> value_out
);

diagnostic_helper!(
    jit_native_binary_diagnostic_abi => jit_native_binary_abi,
    "binary",
    (op: u32, lhs: i64, rhs: i64, function: i64, continuation: i64) -> value_out
);

diagnostic_helper!(
    jit_native_compare_diagnostic_abi => jit_native_compare_abi,
    "compare",
    (op: u32, lhs: i64, rhs: i64) -> value_out
);

diagnostic_helper!(
    jit_native_cast_diagnostic_abi => jit_native_cast_abi,
    "cast",
    (op: u32, src: i64) -> value_out
);

diagnostic_helper!(
    jit_native_echo_diagnostic_abi => jit_native_echo_abi,
    "echo",
    (src: i64) -> i32
);

diagnostic_helper!(
    jit_native_local_fetch_diagnostic_abi => jit_native_local_fetch_abi,
    "local_fetch",
    (quiet: u32, value: i64, function: i64, local: i64, file: i64, start: i64) -> value_out
);

diagnostic_helper!(
    jit_native_local_store_diagnostic_abi => jit_native_local_store_abi,
    "local_store",
    (op: u32, current: i64, value: i64, function: i64, local: i64) -> value_out
);

diagnostic_helper!(
    jit_native_value_release_diagnostic_abi => jit_native_value_release_abi,
    "value_release",
    (encoded: i64) -> i32
);

diagnostic_helper!(
    jit_native_reference_bind_diagnostic_abi => jit_native_reference_bind_abi,
    "reference_bind",
    (op: u32, encoded: i64, key: i64, reserved: i64) -> value_out
);

diagnostic_helper!(
    jit_native_return_check_diagnostic_abi => jit_native_return_check_abi,
    "return_check",
    (op: u32, encoded: i64, function: i64) -> value_out
);

diagnostic_helper!(
    jit_native_argument_check_diagnostic_abi => jit_native_argument_check_abi,
    "argument_check",
    (op: u32, encoded: i64, target_function: i64, parameter_flags: i64, caller_function: i64, continuation: i64) -> value_out
);

diagnostic_helper!(
    jit_native_exception_new_diagnostic_abi => jit_native_exception_new_abi,
    "exception_new",
    (op: u32, message: i64, function: i64, continuation: i64) -> value_out
);

diagnostic_helper!(
    jit_native_array_new_diagnostic_abi => jit_native_array_new_abi,
    "array_new",
    (op: u32) -> value_out
);

diagnostic_helper!(
    jit_native_array_insert_diagnostic_abi => jit_native_array_insert_abi,
    "array_insert",
    (append: u32, array: i64, key: i64, value: i64) -> value_out
);

diagnostic_helper!(
    jit_native_array_insert_local_diagnostic_abi => jit_native_array_insert_local_abi,
    "array_insert",
    (append: u32, array: i64, key: i64, value: i64) -> value_out
);

diagnostic_helper!(
    jit_native_object_new_diagnostic_abi => jit_native_object_new_abi,
    "object_new",
    (class: u32) -> value_out
);

diagnostic_helper!(
    jit_native_property_fetch_diagnostic_abi => jit_native_property_fetch_abi,
    "property_fetch",
    (op: u32, object: i64, function: i64, instruction_id: i64) -> value_out
);

diagnostic_helper!(
    jit_native_property_assign_diagnostic_abi => jit_native_property_assign_abi,
    "property_assign",
    (op: u32, object: i64, value: i64, function: i64, instruction_id: i64) -> value_out
);

diagnostic_helper!(
    jit_native_object_clone_diagnostic_abi => jit_native_object_clone_abi,
    "object_clone",
    (op: u32, object: i64) -> value_out
);

diagnostic_helper!(
    jit_native_object_clone_with_diagnostic_abi => jit_native_object_clone_with_abi,
    "object_clone_with",
    (op: u32, object: i64, replacements: i64) -> value_out
);

diagnostic_helper!(
    jit_native_array_fetch_diagnostic_abi => jit_native_array_fetch_abi,
    "array_fetch",
    (quiet: u32, array: i64, key: i64) -> value_out
);

diagnostic_helper!(
    jit_native_array_unset_diagnostic_abi => jit_native_array_unset_abi,
    "array_unset",
    (op: u32, array: i64, key: i64) -> value_out
);

diagnostic_helper!(
    jit_native_array_spread_diagnostic_abi => jit_native_array_spread_abi,
    "array_spread",
    (op: u32, array: i64, source: i64) -> value_out
);

diagnostic_helper!(
    jit_native_foreach_init_diagnostic_abi => jit_native_foreach_init_abi,
    "foreach_init",
    (op: u32, source: i64, function: i64, local: i64) -> value_out
);

diagnostic_helper!(
    jit_native_foreach_next_diagnostic_abi => jit_native_foreach_next_abi,
    "foreach_next",
    (iterator: i64, key_out: *mut i64, value_out: *mut i64, has_out: *mut i64, state_out: *mut php_jit::JitDeoptState) -> i32
);

diagnostic_helper!(
    jit_native_foreach_cleanup_diagnostic_abi => jit_native_foreach_cleanup_abi,
    "foreach_cleanup",
    (iterator: i64) -> i32
);

diagnostic_helper!(
    jit_native_constant_fetch_diagnostic_abi => jit_native_constant_fetch_abi,
    "constant_fetch",
    (op: u32, function: i64, continuation: i64) -> value_out
);

diagnostic_helper!(
    jit_native_truthy_diagnostic_abi => jit_native_truthy_abi,
    "truthy",
    (src: i64, out: *mut i64) -> i32
);

diagnostic_helper!(
    jit_native_type_predicate_diagnostic_abi => jit_native_type_predicate_abi,
    "type_predicate",
    (op: u32, src: i64) -> value_out
);

diagnostic_helper!(
    jit_native_stable_length_diagnostic_abi => jit_native_stable_length_abi,
    "stable_length",
    (op: u32, src: i64, function: i64, continuation: i64) -> value_out
);

diagnostic_helper!(
    jit_native_string_predicate_diagnostic_abi => jit_native_string_predicate_abi,
    "string_predicate",
    (op: u32, haystack_encoded: i64, needle_encoded: i64) -> value_out
);

diagnostic_helper!(
    jit_native_runtime_fatal_diagnostic_abi => jit_native_runtime_fatal_abi,
    "runtime_fatal",
    (function: u32, continuation: u32) -> i32
);
