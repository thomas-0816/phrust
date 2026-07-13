//! Request-local JIT tiering, blacklist, and compile-cache state.

use super::prelude::*;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct JitFunctionKey {
    pub(super) unit: u64,
    pub(super) function: FunctionId,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct JitCompileCacheKey {
    pub(super) function: u32,
    pub(super) ir_fingerprint: u64,
    pub(super) abi_hash: u64,
    pub(super) config_hash: u64,
    pub(super) target_isa: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct JitCompileCacheEntry {
    pub(super) handle: php_jit::JitFunctionHandle,
    pub(super) runtime_layout_epoch: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum JitCompileCacheLookup {
    Hit(php_jit::JitFunctionHandle),
    Miss,
    Invalidated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum JitBlacklistReason {
    TooManySideExits,
    GuardFailureRate,
    CompileErrors,
    AbiMismatch,
}

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
    pub(super) cooldown_until_call: u64,
    pub(super) blacklist_epoch: u64,
    pub(super) runtime_epoch: u64,
    pub(super) unsupported_epoch: Option<u64>,
    pub(super) blacklist_reason: Option<JitBlacklistReason>,
    pub(super) handle: Option<php_jit::JitFunctionHandle>,
}

impl JitFunctionState {
    pub(super) fn blacklist_strict(&mut self, reason: JitBlacklistReason) -> bool {
        if self.blacklisted {
            return false;
        }
        self.blacklisted = true;
        self.disabled = true;
        self.blacklist_reason = Some(reason);
        true
    }

    fn cooldown(&mut self, reason: JitBlacklistReason, runtime_epoch: u64) -> bool {
        if self.blacklisted {
            return false;
        }
        self.blacklisted = true;
        self.blacklist_reason = Some(reason);
        self.blacklist_epoch = runtime_epoch;
        self.cooldown_until_call = self.calls.saturating_add(JIT_TIERING_COOLDOWN_CALLS);
        true
    }

    pub(super) fn allows_execution(&mut self, runtime_epoch: u64) -> bool {
        if self.disabled {
            return false;
        }
        if !self.blacklisted {
            return true;
        }
        if runtime_epoch == self.blacklist_epoch && self.calls < self.cooldown_until_call {
            return false;
        }
        self.blacklisted = false;
        self.blacklist_reason = None;
        self.side_exits = 0;
        self.guard_failures = 0;
        true
    }

    pub(super) fn record_compile_error(&mut self) -> Option<JitBlacklistReason> {
        self.compile_errors = self.compile_errors.saturating_add(1);
        if self.compile_errors >= JIT_BLACKLIST_COMPILE_ERROR_THRESHOLD
            && self.blacklist_strict(JitBlacklistReason::CompileErrors)
        {
            return Some(JitBlacklistReason::CompileErrors);
        }
        None
    }

    pub(super) fn record_side_exit(
        &mut self,
        reason: php_jit::SideExitReason,
        runtime_epoch: u64,
    ) -> Option<JitBlacklistReason> {
        self.side_exits = self.side_exits.saturating_add(1);
        match reason {
            php_jit::SideExitReason::AbiMismatch => {
                self.abi_mismatches = self.abi_mismatches.saturating_add(1);
                if self.abi_mismatches >= JIT_BLACKLIST_ABI_MISMATCH_THRESHOLD
                    && self.blacklist_strict(JitBlacklistReason::AbiMismatch)
                {
                    return Some(JitBlacklistReason::AbiMismatch);
                }
            }
            php_jit::SideExitReason::GuardFailed => {
                self.guard_failures = self.guard_failures.saturating_add(1);
            }
            _ => {}
        }
        if self.calls >= JIT_TIERING_MIN_EXECUTIONS && self.side_exits >= JIT_TIERING_MIN_SIDE_EXITS
        {
            let excessive_guard_rate = self.guard_failures >= JIT_TIERING_MIN_SIDE_EXITS
                && self.guard_failures.saturating_mul(100)
                    >= self.calls.saturating_mul(JIT_TIERING_MAX_EXIT_RATE_PERCENT);
            let excessive_exit_rate = self.side_exits.saturating_mul(100)
                >= self.calls.saturating_mul(JIT_TIERING_MAX_EXIT_RATE_PERCENT);
            let reason = excessive_guard_rate
                .then_some(JitBlacklistReason::GuardFailureRate)
                .or_else(|| excessive_exit_rate.then_some(JitBlacklistReason::TooManySideExits));
            if let Some(reason) = reason
                && self.cooldown(reason, runtime_epoch)
            {
                return Some(reason);
            }
        }
        None
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct JitRuntimeState {
    pub(super) functions: HashMap<JitFunctionKey, JitFunctionState>,
    pub(super) compile_cache: HashMap<JitCompileCacheKey, JitCompileCacheEntry>,
}

impl JitRuntimeState {
    pub(super) fn lookup_compile_cache(
        &mut self,
        key: &JitCompileCacheKey,
        runtime_layout_epoch: u64,
    ) -> JitCompileCacheLookup {
        let Some(entry) = self.compile_cache.get(key) else {
            return JitCompileCacheLookup::Miss;
        };
        if entry
            .runtime_layout_epoch
            .is_some_and(|compiled_epoch| compiled_epoch != runtime_layout_epoch)
        {
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
        let runtime_layout_epoch = handle
            .property_load_metadata()
            .is_some()
            .then_some(runtime_layout_epoch);
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

pub(super) fn jit_leaf_call_shape_is_supported(
    function: &IrFunction,
    call_shape_supported: bool,
    args: &JitArgumentSlots<'_>,
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
        && !args.has_reference()
}

pub(super) fn native_leaf_rejection_reason(
    function: &IrFunction,
    call_shape_supported: bool,
    args: &JitArgumentSlots<'_>,
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
    if args.has_reference() {
        return "reference_arg";
    }
    "unsupported_leaf_shape"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_exit_policy_requires_samples_and_reenters_after_cooldown() {
        let mut state = JitFunctionState::default();
        for call in 1..JIT_TIERING_MIN_EXECUTIONS {
            state.calls = call;
            assert_eq!(
                state.record_side_exit(php_jit::SideExitReason::TypeMismatch, 7),
                None
            );
            assert!(!state.blacklisted);
        }

        state.calls = JIT_TIERING_MIN_EXECUTIONS;
        assert_eq!(
            state.record_side_exit(php_jit::SideExitReason::TypeMismatch, 7),
            Some(JitBlacklistReason::TooManySideExits)
        );
        assert!(!state.allows_execution(7));

        state.calls = state.cooldown_until_call;
        assert!(state.allows_execution(7));
        assert!(!state.blacklisted);
        assert_eq!(state.side_exits, 0);
    }

    #[test]
    fn epoch_change_ends_dynamic_cooldown_but_abi_mismatch_stays_strict() {
        let mut dynamic = JitFunctionState {
            calls: JIT_TIERING_MIN_EXECUTIONS,
            ..JitFunctionState::default()
        };
        for _ in 0..(JIT_TIERING_MIN_EXECUTIONS / 2) {
            let _ = dynamic.record_side_exit(php_jit::SideExitReason::GuardFailed, 3);
        }
        assert!(dynamic.blacklisted);
        assert!(dynamic.allows_execution(4));

        let mut strict = JitFunctionState::default();
        assert_eq!(
            strict.record_side_exit(php_jit::SideExitReason::AbiMismatch, 3),
            Some(JitBlacklistReason::AbiMismatch)
        );
        assert!(!strict.allows_execution(4));
    }
}
