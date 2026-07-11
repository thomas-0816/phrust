//! Request-local JIT tiering, blacklist, and compile-cache state.

use super::prelude::*;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct JitFunctionKey {
    pub(super) unit: u64,
    pub(super) function: FunctionId,
}

#[cfg(feature = "jit-cranelift")]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct JitCompileCacheKey {
    pub(super) function: u32,
    pub(super) ir_fingerprint: u64,
    pub(super) abi_hash: u64,
    pub(super) config_hash: u64,
    pub(super) target_isa: String,
}

#[cfg(feature = "jit-cranelift")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct JitCompileCacheEntry {
    pub(super) handle: php_jit::JitFunctionHandle,
    pub(super) runtime_layout_epoch: u64,
}

#[cfg(feature = "jit-cranelift")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum JitCompileCacheLookup {
    Hit(php_jit::JitFunctionHandle),
    Miss,
    Invalidated,
}

#[cfg(feature = "jit-cranelift")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum JitBlacklistReason {
    TooManySideExits,
    GuardFailureRate,
    CompileErrors,
    AbiMismatch,
}

#[cfg(feature = "jit-cranelift")]
impl JitBlacklistReason {
    #[must_use]
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::TooManySideExits => "too_many_side_exits",
            Self::GuardFailureRate => "guard_failure_rate",
            Self::CompileErrors => "compile_errors",
            Self::AbiMismatch => "abi_mismatch",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct JitFunctionState {
    pub(super) calls: u64,
    pub(super) compiled: bool,
    pub(super) disabled: bool,
    pub(super) side_exits: u64,
    pub(super) guard_failures: u64,
    pub(super) compile_errors: u64,
    pub(super) abi_mismatches: u64,
    pub(super) blacklisted: bool,
    #[cfg(feature = "jit-cranelift")]
    pub(super) blacklist_reason: Option<JitBlacklistReason>,
    #[cfg(feature = "jit-cranelift")]
    pub(super) handle: Option<php_jit::JitFunctionHandle>,
}

impl JitFunctionState {
    #[cfg(feature = "jit-cranelift")]
    pub(super) fn blacklist(&mut self, reason: JitBlacklistReason) -> bool {
        if self.blacklisted {
            return false;
        }
        self.blacklisted = true;
        self.disabled = true;
        self.blacklist_reason = Some(reason);
        true
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_compile_error(&mut self) -> Option<JitBlacklistReason> {
        self.compile_errors = self.compile_errors.saturating_add(1);
        if self.compile_errors >= JIT_BLACKLIST_COMPILE_ERROR_THRESHOLD
            && self.blacklist(JitBlacklistReason::CompileErrors)
        {
            return Some(JitBlacklistReason::CompileErrors);
        }
        None
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_side_exit(
        &mut self,
        reason: php_jit::SideExitReason,
    ) -> Option<JitBlacklistReason> {
        self.side_exits = self.side_exits.saturating_add(1);
        match reason {
            php_jit::SideExitReason::AbiMismatch => {
                self.abi_mismatches = self.abi_mismatches.saturating_add(1);
                if self.abi_mismatches >= JIT_BLACKLIST_ABI_MISMATCH_THRESHOLD
                    && self.blacklist(JitBlacklistReason::AbiMismatch)
                {
                    return Some(JitBlacklistReason::AbiMismatch);
                }
            }
            php_jit::SideExitReason::GuardFailed => {
                self.guard_failures = self.guard_failures.saturating_add(1);
                if self.guard_failures >= JIT_BLACKLIST_GUARD_FAILURE_THRESHOLD
                    && self.blacklist(JitBlacklistReason::GuardFailureRate)
                {
                    return Some(JitBlacklistReason::GuardFailureRate);
                }
            }
            _ => {}
        }
        if self.side_exits >= JIT_BLACKLIST_SIDE_EXIT_THRESHOLD
            && self.blacklist(JitBlacklistReason::TooManySideExits)
        {
            return Some(JitBlacklistReason::TooManySideExits);
        }
        None
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct JitRuntimeState {
    pub(super) functions: HashMap<JitFunctionKey, JitFunctionState>,
    #[cfg(feature = "jit-cranelift")]
    pub(super) compile_cache: HashMap<JitCompileCacheKey, JitCompileCacheEntry>,
}

#[cfg(feature = "jit-cranelift")]
impl JitRuntimeState {
    pub(super) fn lookup_compile_cache(
        &mut self,
        key: &JitCompileCacheKey,
        runtime_layout_epoch: u64,
    ) -> JitCompileCacheLookup {
        let Some(entry) = self.compile_cache.get(key) else {
            return JitCompileCacheLookup::Miss;
        };
        if entry.runtime_layout_epoch != runtime_layout_epoch {
            self.compile_cache.remove(key);
            return JitCompileCacheLookup::Invalidated;
        }
        JitCompileCacheLookup::Hit(entry.handle.clone())
    }

    pub(super) fn insert_compile_cache(
        &mut self,
        key: JitCompileCacheKey,
        handle: php_jit::JitFunctionHandle,
        runtime_layout_epoch: u64,
    ) {
        self.compile_cache.insert(
            key,
            JitCompileCacheEntry {
                handle,
                runtime_layout_epoch,
            },
        );
    }

    pub(super) fn invalidate_compile_cache_for_function(&mut self, function: FunctionId) -> u64 {
        let before = self.compile_cache.len();
        self.compile_cache
            .retain(|key, _| key.function != function.raw());
        before.saturating_sub(self.compile_cache.len()) as u64
    }
}

#[cfg(feature = "jit-cranelift")]
pub(super) fn jit_leaf_call_shape_is_supported(
    function: &IrFunction,
    call_shape_supported: bool,
    args: &[PreparedArg],
) -> bool {
    call_shape_supported
        && !function.flags.is_top_level
        && !function.flags.is_closure
        && !function.flags.is_method
        && !function.flags.is_generator
        && !function.returns_by_ref
        && matches!(
            function.return_type.as_ref(),
            None | Some(IrReturnType::Int | IrReturnType::String)
        )
        && function.captures.is_empty()
        && function.params.iter().all(|param| {
            !param.by_ref
                && !param.variadic
                && param.default.is_none()
                && matches!(
                    param.type_.as_ref(),
                    None | Some(
                        IrReturnType::Int
                            | IrReturnType::String
                            | IrReturnType::Array
                            | IrReturnType::Class { .. }
                    )
                )
        })
        && args.iter().all(|arg| arg.reference.is_none())
}

#[cfg(feature = "jit-cranelift")]
pub(super) fn native_leaf_rejection_reason(
    function: &IrFunction,
    call_shape_supported: bool,
    args: &[PreparedArg],
) -> &'static str {
    if !call_shape_supported {
        return "call_shape";
    }
    if function.flags.is_top_level {
        return "top_level_function";
    }
    if function.flags.is_closure {
        return "closure";
    }
    if function.flags.is_method {
        return "method";
    }
    if function.flags.is_generator {
        return "generator";
    }
    if function.returns_by_ref {
        return "by_reference_return";
    }
    if !matches!(
        function.return_type.as_ref(),
        None | Some(IrReturnType::Int | IrReturnType::String)
    ) {
        return "return_type";
    }
    if !function.captures.is_empty() {
        return "captured_variables";
    }
    if function.params.iter().any(|param| param.by_ref) {
        return "by_reference_param";
    }
    if function.params.iter().any(|param| param.variadic) {
        return "variadic_param";
    }
    if function.params.iter().any(|param| param.default.is_some()) {
        return "default_param";
    }
    if function.params.iter().any(|param| {
        !matches!(
            param.type_.as_ref(),
            None | Some(
                IrReturnType::Int
                    | IrReturnType::String
                    | IrReturnType::Array
                    | IrReturnType::Class { .. }
            )
        )
    }) {
        return "param_type";
    }
    if args.iter().any(|arg| arg.reference.is_some()) {
        return "reference_arg";
    }
    "unsupported_leaf_shape"
}
