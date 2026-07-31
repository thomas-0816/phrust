/// Fixed variadic ABI for coordinated native array sorting.
///
/// The argument pointer addresses synchronous generated stack storage. Every
/// element is an authoritative native encoding; this handler never constructs
/// a Rust `Value` or enters builtin dispatch.
pub(crate) extern "C" fn jit_native_array_multisort_abi(
    runtime: *mut NativeRequestFastState,
    argument_count: u32,
    arguments: *const i64,
) -> php_jit::JitNativeControlResult {
    let Ok(argument_count) = usize::try_from(argument_count) else {
        return exact_query_contract_violation();
    };
    if argument_count == 0 || arguments.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: generated code owns this stack slice for the duration of this
    // synchronous fixed native call and passes its exact element count.
    #[allow(unsafe_code)] // Safety: generated code keeps the exact argument slice live for this call.
    let arguments = unsafe { std::slice::from_raw_parts(arguments, argument_count) };
    // Safety: the generated ABI passes the active request's fast state.
    #[allow(unsafe_code)] // Safety: generated code passes the active request-owned fast state.
    let fast = unsafe { &mut *runtime };
    if fast.native_array_multisort(arguments).is_none() {
        return exact_query_contract_violation();
    }
    exact_query_return_bool(true)
}
