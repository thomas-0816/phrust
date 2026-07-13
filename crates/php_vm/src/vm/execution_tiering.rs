use super::*;

pub(super) struct JitLeafRequest<'a> {
    pub(super) compiled: &'a CompiledUnit,
    pub(super) state: &'a ExecutionState,
    pub(super) function_id: FunctionId,
    pub(super) function: &'a IrFunction,
    pub(super) tier: ExecutionTier,
    pub(super) call_shape_supported: bool,
    pub(super) args: JitArgumentSlots<'a>,
}

pub(super) enum JitArgumentSlots<'a> {
    Prepared(&'a [PreparedArg]),
    DirectFrame(&'a Frame),
}

impl JitArgumentSlots<'_> {
    fn len(&self, function: &IrFunction) -> usize {
        match self {
            Self::Prepared(args) => args.len(),
            Self::DirectFrame(_) => function.params.len(),
        }
    }

    fn value<'a>(&'a self, function: &IrFunction, index: usize) -> Option<&'a Value> {
        match self {
            Self::Prepared(args) => args.get(index).map(|arg| &arg.value),
            Self::DirectFrame(frame) => {
                let param = function.params.get(index)?;
                match frame.locals.get_slot(param.local)? {
                    Slot::Value(value) => Some(value),
                    Slot::Reference(_) => None,
                }
            }
        }
    }

    pub(super) fn has_reference(&self) -> bool {
        match self {
            Self::Prepared(args) => args.iter().any(|arg| arg.reference.is_some()),
            Self::DirectFrame(frame) => frame
                .locals
                .iter()
                .any(|(_, slot)| matches!(slot, Slot::Reference(_))),
        }
    }
}

impl Vm {
    #[cfg(feature = "jit-cranelift")]
    pub(super) fn try_execute_dense_jit_leaf(
        &self,
        compiled: &CompiledUnit,
        state: &ExecutionState,
        function_id: FunctionId,
        function: &IrFunction,
        call: &FunctionCall<'_>,
    ) -> Option<Value> {
        let tier = self.tiering.borrow_mut().record_function_entry(
            compiled_unit_cache_key(compiled),
            function_id,
            self.options.quickening,
            self.options.jit,
        );
        self.record_counter_jit_tiering_decision(tier);

        // The dense direct-call lane has already proved exact arity and plain
        // positional binding. Keep the JIT adapter stack-inline and leave all
        // richer shapes to the normal binder/interpreter path.
        if call.arg_count() != function.params.len() || call.arg_count() > 8 {
            return None;
        }
        let call_shape_supported = call.captures.is_empty()
            && call.args.iter().all(|arg| {
                arg.name.is_none()
                    && arg.by_ref_dim.is_none()
                    && arg.by_ref_property.is_none()
                    && arg.by_ref_property_dim.is_none()
            })
            && call.this_value.is_none()
            && call.scope_class.is_none()
            && call.called_class.is_none()
            && call.declaring_class.is_none()
            && call.shared_top_level_locals.is_none()
            && call.running_generator.is_none()
            && call.running_fiber.is_none();
        let mut args = smallvec::SmallVec::<[PreparedArg; 8]>::new();
        args.extend(call.args.iter().map(|arg| PreparedArg {
            value: match &arg.value {
                Value::Reference(cell) => cell.get(),
                value => value.clone(),
            },
            reference: None,
            trace_holds_reference: false,
        }));

        // Rich dispatch invokes Cranelift only after prepare_arguments. Apply
        // the same scalar coercion/type checks here before native execution so
        // weak typing and strict_types cannot diverge on the dense lane.
        let strict_types = call
            .argument_binding_policy(compiled)
            .call_site_strict_types;
        for (arg_index, (arg, param)) in args.iter_mut().zip(&function.params).enumerate() {
            if coerce_or_check_param_type(
                ParamTypecheckRequest {
                    compiled,
                    state,
                    function,
                    param,
                    arg_index,
                    fast_path: self.typecheck_fast_path_context(),
                    strict_types,
                    call_span: call.call_span,
                },
                &mut arg.value,
            )
            .is_err()
            {
                return None;
            }
        }

        self.try_execute_jit_leaf(JitLeafRequest {
            compiled,
            state,
            function_id,
            function,
            tier,
            call_shape_supported,
            args: JitArgumentSlots::Prepared(&args),
        })
    }

    #[cfg(not(feature = "jit-cranelift"))]
    pub(super) fn try_execute_jit_leaf(&self, _request: JitLeafRequest<'_>) -> Option<Value> {
        None
    }

    #[cfg(feature = "jit-cranelift")]
    // Audited native-tier helper boundary (docs/performance/cranelift/
    // safety-audit.md): reconstitutes Box<Value> pointers produced by JIT
    // helpers for this synchronous call.
    #[allow(unsafe_code)]
    pub(super) fn try_execute_jit_leaf(&self, request: JitLeafRequest<'_>) -> Option<Value> {
        let JitLeafRequest {
            compiled,
            state,
            function_id,
            function,
            tier,
            call_shape_supported,
            args,
        } = request;
        if tier != ExecutionTier::Jit || !self.options.tiering.enabled {
            return None;
        }
        if self.options.jit != JitMode::Cranelift {
            return None;
        }
        self.record_counter_native_candidate();
        if !jit_leaf_call_shape_is_supported(function, call_shape_supported, &args) {
            let reason = native_leaf_rejection_reason(function, call_shape_supported, &args);
            self.record_counter_native_eligibility_rejection(reason);
            return None;
        }

        let key = JitFunctionKey {
            unit: compiled_unit_cache_key(compiled),
            function: function_id,
        };
        let cache_key = jit_compile_cache_key(function_id, function, &self.options);
        let runtime_layout_epoch = state.lookup_epoch().raw();
        {
            let mut jit = self.jit.borrow_mut();
            let entry = jit.functions.entry(key).or_default();
            entry.calls = entry.calls.saturating_add(1);
            entry.runtime_epoch = runtime_layout_epoch;
            if entry.unsupported_epoch == Some(runtime_layout_epoch) {
                return None;
            }
            if entry.unsupported_epoch.is_some() {
                entry.unsupported_epoch = None;
            }
            if (self.options.jit_blacklist.enabled()
                && !entry.allows_execution(runtime_layout_epoch))
                || entry.disabled
            {
                self.record_counter_jit_tiering_blacklist_rejection();
                return None;
            }
        }

        let cache_lookup = self
            .jit
            .borrow_mut()
            .lookup_compile_cache(&cache_key, runtime_layout_epoch);
        let handle = match cache_lookup {
            JitCompileCacheLookup::Hit(handle) => {
                self.record_counter_jit_compile_cache_hit();
                if let Some(entry) = self.jit.borrow_mut().functions.get_mut(&key) {
                    entry.compiled = true;
                    entry.handle = Some(handle.clone());
                }
                handle
            }
            JitCompileCacheLookup::Invalidated => {
                self.record_counter_jit_compile_cache_invalidations(1);
                self.record_counter_jit_compile_cache_miss();
                if let Some(entry) = self.jit.borrow_mut().functions.get_mut(&key) {
                    entry.compiled = false;
                    entry.handle = None;
                }
                self.compile_cranelift_jit_leaf(
                    compiled,
                    function_id,
                    function,
                    key,
                    cache_key,
                    runtime_layout_epoch,
                )?
            }
            JitCompileCacheLookup::Miss => {
                self.record_counter_jit_compile_cache_miss();
                self.compile_cranelift_jit_leaf(
                    compiled,
                    function_id,
                    function,
                    key,
                    cache_key,
                    runtime_layout_epoch,
                )?
            }
        };

        if handle.expects_value_metadata() {
            let Some(object_arg) = (args.len(function) == 1)
                .then(|| args.value(function, 0))
                .flatten()
            else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                self.record_counter_property_load_guard_exit();
                self.record_counter_property_load_slow_call();
                return None;
            };
            let Some(metadata) = handle.property_load_metadata() else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::GuardFailed),
                );
                self.record_counter_jit_guard_failure();
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                self.record_counter_property_load_guard_exit();
                self.record_counter_property_load_slow_call();
                return None;
            };
            if let Some(status) =
                property_load_pre_guard_status(compiled, state, object_arg, metadata)
            {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::GuardFailed)
                        .with_status(status),
                );
                self.record_counter_jit_guard_failure();
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                self.record_counter_property_load_guard_exit();
                self.record_counter_property_load_slow_call();
                if status == JIT_PROPERTY_LOAD_STATUS_LAYOUT_EXIT {
                    self.record_counter_property_load_layout_exit();
                }
                return None;
            }
            let value_ptr = object_arg as *const Value as usize;
            let metadata_ptr = metadata as *const php_jit::JitPropertyLoadMetadata as usize;
            match handle.invoke_value_metadata(
                value_ptr,
                metadata_ptr,
                php_jit::JIT_RUNTIME_ABI_HASH,
            ) {
                Ok(value_ptr) if value_ptr != 0 => {
                    // SAFETY: Successful property-load helpers return a pointer
                    // created with `Box::into_raw(Box<Value>)` specifically for
                    // this synchronous VM call.
                    let value = unsafe { *Box::from_raw(value_ptr as *mut Value) };
                    self.record_counter_jit_helper_calls(handle.helper_calls_per_invocation());
                    self.record_counter_jit_fast_path_hits(handle.fast_path_hits_per_invocation());
                    self.record_counter_property_load_fast_hit();
                    self.record_counter_jit_executed();
                    return Some(value);
                }
                Ok(_) => {
                    self.record_jit_side_exit_for_key(
                        key,
                        php_jit::JitSideExit::new(php_jit::SideExitReason::HelperStatus),
                    );
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    self.record_counter_property_load_guard_exit();
                    self.record_counter_property_load_slow_call();
                    return None;
                }
                Err(error) => {
                    let status = error.native_status();
                    let side_exit = match status {
                        Some(
                            status @ (JIT_PROPERTY_LOAD_STATUS_CLASS_EXIT
                            | JIT_PROPERTY_LOAD_STATUS_LAYOUT_EXIT
                            | JIT_PROPERTY_LOAD_STATUS_UNINITIALIZED_EXIT
                            | JIT_PROPERTY_LOAD_STATUS_STORAGE_EXIT),
                        ) => php_jit::JitSideExit::new(php_jit::SideExitReason::GuardFailed)
                            .with_status(status),
                        _ => error.side_exit(),
                    };
                    self.record_jit_side_exit_for_key(key, side_exit);
                    self.record_counter_jit_guard_failure();
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    self.record_counter_property_load_guard_exit();
                    self.record_counter_property_load_slow_call();
                    if status == Some(JIT_PROPERTY_LOAD_STATUS_LAYOUT_EXIT) {
                        self.record_counter_property_load_layout_exit();
                    }
                    if status == Some(JIT_PROPERTY_LOAD_STATUS_UNINITIALIZED_EXIT) {
                        self.record_counter_property_load_uninitialized_exit();
                    }
                    return None;
                }
            }
        }

        if handle.expects_value() {
            let Some(array_arg) = (args.len(function) == 1)
                .then(|| args.value(function, 0))
                .flatten()
            else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                return None;
            };
            let value_ptr = array_arg as *const Value as usize;
            match handle.invoke_value(value_ptr, php_jit::JIT_RUNTIME_ABI_HASH) {
                Ok(value) => {
                    self.record_counter_jit_helper_calls(handle.helper_calls_per_invocation());
                    self.record_counter_jit_fast_path_hits(handle.fast_path_hits_per_invocation());
                    match handle.specialization() {
                        php_jit::JitNativeSpecialization::PackedForeachIntSum => {
                            self.record_counter_packed_foreach_sum_fast_hit();
                        }
                        php_jit::JitNativeSpecialization::KnownCallStrlen
                        | php_jit::JitNativeSpecialization::KnownCallCount => {
                            self.record_counter_known_call_fast_hit();
                        }
                        php_jit::JitNativeSpecialization::StringConcat
                        | php_jit::JitNativeSpecialization::PropertyLoad
                        | php_jit::JitNativeSpecialization::RecordArrayLookup
                        | php_jit::JitNativeSpecialization::Generic => {}
                    }
                    self.record_counter_jit_executed();
                    return Some(Value::Int(value));
                }
                Err(error) => {
                    let side_exit = error.side_exit();
                    match handle.specialization() {
                        php_jit::JitNativeSpecialization::PackedForeachIntSum => {
                            match error.native_status() {
                                Some(status) if status == php_jit::JIT_HELPER_STATUS_OVERFLOW => {
                                    self.record_counter_jit_overflow_exit();
                                    self.record_counter_packed_foreach_sum_overflow_exit();
                                }
                                Some(status)
                                    if status
                                        == php_runtime::experimental::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT
                                        || status
                                            == php_runtime::experimental::PHP_JIT_ARRAY_STATUS_FALLBACK =>
                                {
                                    self.record_counter_packed_foreach_sum_layout_exit();
                                }
                                _ => {}
                            }
                        }
                        php_jit::JitNativeSpecialization::KnownCallStrlen
                        | php_jit::JitNativeSpecialization::KnownCallCount => {
                            self.record_counter_known_call_guard_exit();
                            self.record_counter_known_call_slow_call();
                        }
                        php_jit::JitNativeSpecialization::StringConcat
                        | php_jit::JitNativeSpecialization::PropertyLoad
                        | php_jit::JitNativeSpecialization::RecordArrayLookup
                        | php_jit::JitNativeSpecialization::Generic => {}
                    }
                    self.record_jit_side_exit_for_key(key, side_exit);
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    return None;
                }
            }
        }

        if handle.expects_value_value() {
            let (Some(lhs_arg), Some(rhs_arg)) = (args.value(function, 0), args.value(function, 1))
            else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                self.record_counter_string_concat_fast_path(false);
                return None;
            };
            if args.len(function) != 2 {
                return None;
            }
            let lhs_ptr = lhs_arg as *const Value as usize;
            let rhs_ptr = rhs_arg as *const Value as usize;
            match handle.invoke_value_value(lhs_ptr, rhs_ptr, php_jit::JIT_RUNTIME_ABI_HASH) {
                Ok(value_ptr) if value_ptr != 0 => {
                    // SAFETY: Successful value/value helpers return a pointer
                    // created with `Box::into_raw(Box<Value>)` specifically for
                    // this synchronous VM call.
                    let value = unsafe { *Box::from_raw(value_ptr as *mut Value) };
                    self.record_counter_jit_helper_calls(handle.helper_calls_per_invocation());
                    self.record_counter_jit_fast_path_hits(handle.fast_path_hits_per_invocation());
                    match handle.specialization() {
                        php_jit::JitNativeSpecialization::StringConcat => {
                            self.record_counter_string_concat_fast_path(true);
                        }
                        php_jit::JitNativeSpecialization::RecordArrayLookup => {
                            self.record_counter_record_lookup_fast_hit();
                        }
                        _ => {}
                    }
                    self.record_counter_jit_executed();
                    return Some(value);
                }
                Ok(_) => {
                    self.record_jit_side_exit_for_key(
                        key,
                        php_jit::JitSideExit::new(php_jit::SideExitReason::HelperStatus),
                    );
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    if handle.specialization() == php_jit::JitNativeSpecialization::StringConcat {
                        self.record_counter_string_concat_fast_path(false);
                    }
                    return None;
                }
                Err(error) => {
                    let side_exit = error.side_exit();
                    self.record_jit_side_exit_for_key(key, side_exit);
                    if matches!(
                        error.native_status(),
                        Some(status) if status == php_jit::JIT_HELPER_STATUS_OVERFLOW
                    ) {
                        self.record_counter_jit_overflow_exit();
                    }
                    match handle.specialization() {
                        php_jit::JitNativeSpecialization::StringConcat => {
                            self.record_counter_string_concat_fast_path(false);
                        }
                        php_jit::JitNativeSpecialization::RecordArrayLookup => {
                            match error.native_status() {
                                Some(status)
                                    if status
                                        == php_runtime::experimental::PHP_JIT_ARRAY_STATUS_KEY_MISS_EXIT =>
                                {
                                    self.record_counter_record_lookup_key_miss_exit();
                                }
                                Some(status)
                                    if status
                                        == php_runtime::experimental::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT =>
                                {
                                    self.record_counter_record_lookup_layout_exit();
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    return None;
                }
            }
        }

        if handle.expects_value_i64() {
            let (Some(array_arg), Some(index_arg)) =
                (args.value(function, 0), args.value(function, 1))
            else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                return None;
            };
            if args.len(function) != 2 {
                return None;
            }
            let Value::Int(index) = index_arg else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                return None;
            };
            let value_ptr = array_arg as *const Value as usize;
            match handle.invoke_value_i64(value_ptr, *index, php_jit::JIT_RUNTIME_ABI_HASH) {
                Ok(value) => {
                    self.record_counter_jit_helper_calls(handle.helper_calls_per_invocation());
                    self.record_counter_jit_fast_path_hits(handle.fast_path_hits_per_invocation());
                    self.record_counter_packed_fetch_fast_hit();
                    self.record_counter_jit_executed();
                    return Some(Value::Int(value));
                }
                Err(error) => {
                    let mut side_exit = error.side_exit();
                    match error.native_status() {
                        Some(status)
                            if status
                                == php_runtime::experimental::PHP_JIT_ARRAY_STATUS_BOUNDS_EXIT =>
                        {
                            self.record_counter_packed_fetch_bounds_exit();
                            side_exit =
                                php_jit::JitSideExit::new(php_jit::SideExitReason::HelperStatus)
                                    .with_status(status);
                        }
                        Some(status)
                            if status
                                == php_runtime::experimental::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT =>
                        {
                            self.record_counter_packed_fetch_layout_exit();
                        }
                        _ => {}
                    }
                    self.record_jit_side_exit_for_key(key, side_exit);
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    return None;
                }
            }
        }

        let mut native_args = smallvec::SmallVec::<[i64; 8]>::new();
        for index in 0..args.len(function) {
            let Some(value) = args.value(function, index) else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                return None;
            };
            let Ok(value) = value_as_jit_int(value) else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                return None;
            };
            native_args.push(value);
        }
        if matches!(args, JitArgumentSlots::DirectFrame(_)) {
            self.record_counter_cranelift_direct_slot_marshal(native_args.len());
        } else {
            self.record_counter_cranelift_prepared_arg_materialization();
        }
        match handle.invoke_i64(&native_args, php_jit::JIT_RUNTIME_ABI_HASH) {
            Ok(value) => {
                self.record_counter_jit_helper_calls(handle.helper_calls_per_invocation());
                self.record_counter_jit_fast_path_hits(handle.fast_path_hits_per_invocation());
                self.record_counter_compiled_to_compiled_calls(
                    handle.compiled_to_compiled_calls_per_invocation(),
                );
                self.record_counter_jit_executed();
                Some(Value::Int(value))
            }
            Err(error) => {
                let side_exit = error.side_exit();
                if side_exit.reason == php_jit::SideExitReason::Overflow {
                    self.record_counter_jit_overflow_exit();
                }
                self.record_jit_side_exit_for_key(key, side_exit);
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                None
            }
        }
    }

    #[cfg(feature = "jit-cranelift")]
    /// Precompiles a bounded set of eligible functions without executing the
    /// script. Published code is retained by the process code manager and can
    /// be adopted by every worker that later requests the same cache key.
    pub fn prewarm_cranelift(&self, compiled: &CompiledUnit) -> u64 {
        if self.options.jit != JitMode::Cranelift || !self.options.tiering.enabled {
            return 0;
        }
        const MAX_FUNCTIONS: usize = 64;
        const MAX_TIME: Duration = Duration::from_millis(10);
        let started = Instant::now();
        let runtime_epoch = if self.options.worker_symbol_epoch {
            WORKER_SYMBOL_LEDGER.with(|ledger| ledger.epoch.get())
        } else {
            0
        };
        let mut compiled_count = 0_u64;
        for (index, function) in compiled
            .unit()
            .functions
            .iter()
            .enumerate()
            .take(MAX_FUNCTIONS)
        {
            if started.elapsed() >= MAX_TIME {
                break;
            }
            let function_id = FunctionId::new(index as u32);
            let key = JitFunctionKey {
                unit: compiled_unit_cache_key(compiled),
                function: function_id,
            };
            let cache_key = jit_compile_cache_key(function_id, function, &self.options);
            if matches!(
                self.jit
                    .borrow_mut()
                    .lookup_compile_cache(&cache_key, runtime_epoch),
                JitCompileCacheLookup::Hit(_)
            ) {
                continue;
            }
            if self
                .compile_cranelift_jit_leaf(
                    compiled,
                    function_id,
                    function,
                    key,
                    cache_key,
                    runtime_epoch,
                )
                .is_some()
            {
                compiled_count = compiled_count.saturating_add(1);
            }
        }
        compiled_count
    }

    #[cfg(feature = "jit-cranelift")]
    fn compile_cranelift_jit_leaf(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        function: &IrFunction,
        key: JitFunctionKey,
        cache_key: JitCompileCacheKey,
        runtime_layout_epoch: u64,
    ) -> Option<php_jit::JitFunctionHandle> {
        if !self.jit_compile_budget_allows_attempt() {
            self.record_counter_jit_tiering_budget_rejection();
            return None;
        }
        self.record_counter_jit_compile_attempt();
        let mut engine = php_jit::JitEngine::with_options(php_jit::JitOptions {
            enabled: true,
            allow_native_execution: true,
        });
        let compile_result = engine.compile_function_with_runtime_helpers(
            compiled.unit(),
            function_id,
            php_jit::JitCompileRequest::new(format!("function.{}", function.name))
                .with_function_name(function.name.clone())
                .with_ir_fingerprint(format!("{:016x}", cache_key.ir_fingerprint))
                .with_config_hash(cache_key.config_hash)
                .with_invalidation_generation(runtime_layout_epoch),
            php_jit::JitRuntimeHelperAddresses {
                helper_table: jit_runtime_helper_table() as *const _ as usize,
                packed_array_len: jit_array_len_abi as *const () as usize,
                packed_array_fetch_int_slow: jit_array_fetch_int_slow_abi as *const () as usize,
                known_strlen: jit_strlen_known_abi as *const () as usize,
                known_count: jit_count_known_abi as *const () as usize,
                string_concat: jit_concat_string_string_fast as *const () as usize,
                property_load: jit_property_load_monomorphic_fast as *const () as usize,
                record_array_lookup: jit_record_array_lookup_abi as *const () as usize,
            },
        );
        if let Ok(result) = &compile_result
            && let Some(event) = result
                .handle
                .as_ref()
                .and_then(php_jit::JitFunctionHandle::code_manager_event)
        {
            self.record_counter_jit_code_manager_event(event);
        }
        match compile_result {
            Ok(result) if result.status == php_jit::JitCompileStatus::Compiled => {
                let Some(mut handle) = result.handle else {
                    self.record_jit_compile_failure_for_key(key);
                    self.record_counter_jit_bailout();
                    return None;
                };
                handle.bind_runtime_layout_version(runtime_layout_epoch);
                let descriptor = JitCompileDescriptor {
                    function_id: function_id.raw(),
                    function_name: function.name.clone(),
                    ir_fingerprint: format!("{:016x}", cache_key.ir_fingerprint),
                    code_bytes: result.stats.native_code_bytes,
                    compile_time_nanos: result.stats.native_compile_time_nanos,
                    target_isa: cache_key.target_isa.clone(),
                    abi_hash: cache_key.abi_hash,
                    config_hash: cache_key.config_hash,
                };
                {
                    let mut jit = self.jit.borrow_mut();
                    if let Some(entry) = jit.functions.get_mut(&key) {
                        entry.compiled = true;
                        entry.handle = Some(handle.clone());
                    }
                    jit.insert_compile_cache(cache_key, handle.clone(), runtime_layout_epoch);
                }
                self.record_counter_jit_compiled();
                self.record_counter_jit_compile_metadata(
                    result.stats.native_code_bytes,
                    result.stats.native_compile_time_nanos,
                );
                self.record_counter_jit_compile_descriptor(descriptor);
                self.maybe_write_cranelift_clif_dump(compiled, function_id);
                self.record_jit_compile_budget_spent(result.stats.native_compile_time_nanos);
                Some(handle)
            }
            Ok(_) => {
                // A normal unsupported PHP shape is not a compiler failure and
                // must remain eligible for a later specialization/version.
                if let Some(entry) = self.jit.borrow_mut().functions.get_mut(&key) {
                    entry.unsupported_epoch = Some(runtime_layout_epoch);
                }
                self.record_counter_jit_bailout();
                None
            }
            Err(_) => {
                self.record_jit_compile_failure_for_key(key);
                self.record_counter_jit_bailout();
                None
            }
        }
    }

    pub(super) fn try_execute_bytecode_entry(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> BytecodeEntryAttempt {
        match self.try_execute_dense_function_entry(
            compiled,
            compiled.unit().entry,
            FunctionCall::new(Vec::new(), Vec::new()),
            output,
            stack,
            state,
        ) {
            BytecodeFunctionAttempt::Executed(result, _) => BytecodeEntryAttempt::Executed(result),
            BytecodeFunctionAttempt::Unsupported(message, _) => {
                BytecodeEntryAttempt::Unsupported(message)
            }
        }
    }

    fn dense_execution_artifact_key(&self) -> DenseExecutionArtifactKey {
        DenseExecutionArtifactKey {
            mode: if self.options.execution_format.is_strict_bytecode() {
                DenseExecutionArtifactMode::Strict
            } else {
                DenseExecutionArtifactMode::Mixed
            },
            superinstructions: self.options.superinstructions.is_enabled(),
            profiled_layout: self.options.bytecode_layout.is_profiled(),
            layout_profile_entries: if self.options.bytecode_layout.is_profiled() {
                self.options
                    .bytecode_layout_profile
                    .as_ref()
                    .map(|profile| {
                        profile
                            .block_entries
                            .iter()
                            .map(|(key, value)| (key.clone(), *value))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            },
            dense_jump_threading: self.options.dense_jump_threading.is_enabled(),
        }
    }

    pub(super) fn get_or_build_dense_execution_plan(
        &self,
        compiled: &CompiledUnit,
    ) -> Result<Arc<DenseExecutionPlan>, String> {
        let key = DenseExecutionPlanThreadCacheKey {
            compiled_identity: compiled.cache_identity(),
            artifact: self.dense_execution_artifact_key(),
        };
        if let Some(plan) =
            DENSE_EXECUTION_PLAN_THREAD_CACHE.with(|cache| cache.borrow().get(&key).cloned())
        {
            self.record_counter_dense_execution_plan_cache_hit();
            self.record_counter_dense_execution_plan(plan.as_ref());
            return Ok(plan);
        }

        #[allow(clippy::arc_with_non_send_sync)] // plan sharing predates a Send-safe design
        let plan = Arc::new({
            let mut plan = self.build_dense_execution_plan(compiled)?;
            plan.call_shape_meta = dense_call_shape_meta_for_unit(compiled.unit());
            plan.last_use_plans = (0..plan.functions.len())
                .map(|_| std::cell::OnceCell::new())
                .collect();
            plan
        });
        DENSE_EXECUTION_PLAN_THREAD_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() >= DENSE_EXECUTION_PLAN_THREAD_CACHE_MAX {
                cache.clear();
            }
            cache.insert(key, Arc::clone(&plan));
        });
        self.record_counter_dense_execution_plan_cache_miss();
        self.record_counter_dense_execution_plan(plan.as_ref());
        Ok(plan)
    }

    fn build_dense_execution_plan(
        &self,
        compiled: &CompiledUnit,
    ) -> Result<DenseExecutionPlan, String> {
        self.record_counter_bytecode_lower_attempt();
        if !self.options.execution_format.is_strict_bytecode() {
            let mut plan = DenseBytecodeUnit::mixed_plan_from_ir(compiled.unit());
            if let Err(errors) = plan.unit.verify() {
                return Err(format!(
                    "E_PHP_VM_DENSE_BYTECODE_VERIFY: mixed dense bytecode verifier rejected unit with {} error(s)",
                    errors.len()
                ));
            }
            if self.options.superinstructions.is_enabled() {
                let report = plan.unit.select_superinstructions();
                self.record_counter_superinstruction_selection(&report);
                if let Err(errors) = plan.unit.verify() {
                    return Err(format!(
                        "E_PHP_VM_DENSE_SUPERINSTRUCTION_VERIFY: selected mixed dense bytecode failed verification with {} error(s)",
                        errors.len()
                    ));
                }
            }
            if self.options.bytecode_layout.is_profiled() {
                let _report = plan
                    .unit
                    .apply_profiled_layout(self.options.bytecode_layout_profile.as_ref());
                if let Err(errors) = plan.unit.verify() {
                    return Err(format!(
                        "E_PHP_VM_DENSE_LAYOUT_VERIFY: profiled mixed dense bytecode layout failed verification with {} error(s)",
                        errors.len()
                    ));
                }
            }
            if self.options.dense_jump_threading.is_enabled() && plan.unit.has_jump_trampolines() {
                // Verifier-bracketed with rollback: a threading result that
                // fails verification restores the pre-pass unit instead of
                // dropping the whole plan. The trampoline pre-scan keeps the
                // snapshot clone off the common no-trampoline path.
                let snapshot = plan.unit.clone();
                let report = plan.unit.thread_jump_chains();
                if report.threaded_edges > 0 && plan.unit.verify().is_err() {
                    plan.unit = snapshot;
                    self.record_counter_dense_jump_threading(&report, true);
                } else {
                    self.record_counter_dense_jump_threading(&report, false);
                }
            }
            self.record_counter_bytecode_lowered_families(&plan.unit);
            self.record_counter_bytecode_lower_success();
            return Ok(plan);
        }

        let mut dense = DenseBytecodeUnit::lower_from_ir(compiled.unit())
            .map_err(|error| format!("E_PHP_VM_DENSE_BYTECODE_UNSUPPORTED: {}", error.message))?;
        if let Err(errors) = dense.verify() {
            return Err(format!(
                "E_PHP_VM_DENSE_BYTECODE_VERIFY: dense bytecode verifier rejected unit with {} error(s)",
                errors.len()
            ));
        }
        if self.options.superinstructions.is_enabled() {
            let report = dense.select_superinstructions();
            self.record_counter_superinstruction_selection(&report);
            if let Err(errors) = dense.verify() {
                return Err(format!(
                    "E_PHP_VM_DENSE_SUPERINSTRUCTION_VERIFY: selected dense bytecode failed verification with {} error(s)",
                    errors.len()
                ));
            }
        }
        if self.options.bytecode_layout.is_profiled() {
            let _report =
                dense.apply_profiled_layout(self.options.bytecode_layout_profile.as_ref());
            if let Err(errors) = dense.verify() {
                return Err(format!(
                    "E_PHP_VM_DENSE_LAYOUT_VERIFY: profiled dense bytecode layout failed verification with {} error(s)",
                    errors.len()
                ));
            }
        }
        if self.options.dense_jump_threading.is_enabled() && dense.has_jump_trampolines() {
            let snapshot = dense.clone();
            let report = dense.thread_jump_chains();
            if report.threaded_edges > 0 && dense.verify().is_err() {
                dense = snapshot;
                self.record_counter_dense_jump_threading(&report, true);
            } else {
                self.record_counter_dense_jump_threading(&report, false);
            }
        }
        self.record_counter_bytecode_lowered_families(&dense);
        self.record_counter_bytecode_lower_success();
        let functions = dense
            .functions
            .iter()
            .map(|_| DenseFunctionPlan::Dense)
            .collect();
        Ok(DenseExecutionPlan {
            unit: dense,
            functions,
            call_shape_meta: Vec::new(),
            last_use_plans: Vec::new(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_execute_dense_function_entry<'a>(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        call: FunctionCall<'a>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> BytecodeFunctionAttempt<'a> {
        let mut call = Some(call);
        if self.options.trace || self.options.trace_runtime {
            return BytecodeFunctionAttempt::Unsupported(
                "E_PHP_VM_DENSE_BYTECODE_TRACE_UNSUPPORTED: dense bytecode execution does not support tracing yet"
                    .to_string(),
                call.expect("call should be available before execution starts"),
            );
        }
        let Some(ir_function) = compiled.unit().functions.get(function_id.index()) else {
            return BytecodeFunctionAttempt::Unsupported(
                "E_PHP_VM_DENSE_BYTECODE_ENTRY: IR entry function is missing".to_string(),
                call.expect("call should be available before execution starts"),
            );
        };
        let entry_tier = self.tiering.borrow_mut().record_function_entry(
            compiled_unit_cache_key(compiled),
            function_id,
            self.options.quickening,
            self.options.jit,
        );
        self.record_counter_jit_tiering_decision(entry_tier);
        let plan = match self.get_or_build_dense_execution_plan(compiled) {
            Ok(plan) => plan,
            Err(message) => {
                return BytecodeFunctionAttempt::Unsupported(
                    message,
                    call.expect("call should be available before execution starts"),
                );
            }
        };
        match plan.function_plan(function_id.index()) {
            Some(DenseFunctionPlan::Dense) => {
                let Some(dense_function) = plan.unit.functions.get(function_id.index()) else {
                    return BytecodeFunctionAttempt::Unsupported(
                        "E_PHP_VM_DENSE_BYTECODE_ENTRY: dense bytecode entry function is missing"
                            .to_string(),
                        call.expect("call should be available before execution starts"),
                    );
                };
                BytecodeFunctionAttempt::Executed(
                    Box::new(self.execute_bytecode_function(
                        DenseExecutionRequest {
                            compiled,
                            dense: &plan.unit,
                            plan: Some(plan.as_ref()),
                            dense_function,
                            ir_function,
                            function_id,
                            call: call.take().expect("call should be consumed exactly once"),
                            direct_call: None,
                            resume: None,
                        },
                        output,
                        stack,
                        state,
                    )),
                    BytecodeFunctionTier::Dense,
                )
            }
            Some(DenseFunctionPlan::RichFallback { reason }) => {
                self.record_counter_rich_fallback_function_executed(reason, &ir_function.name);
                BytecodeFunctionAttempt::Executed(
                    Box::new(self.execute_function(
                        compiled,
                        function_id,
                        call.take().expect("call should be consumed exactly once"),
                        output,
                        stack,
                        state,
                    )),
                    BytecodeFunctionTier::RichFallback(reason.clone()),
                )
            }
            None => BytecodeFunctionAttempt::Unsupported(
                "E_PHP_VM_DENSE_BYTECODE_ENTRY: dense execution plan entry is missing".to_string(),
                call.expect("call should be available before execution starts"),
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_execute_cached_dense_function_dispatch<'a>(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        function: &IrFunction,
        call: FunctionCall<'a>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> CachedDenseFunctionDispatch<'a> {
        if !self.options.execution_format.attempts_bytecode()
            || self.options.trace
            || self.options.trace_runtime
            || function.flags.is_generator
            || call.resume_continuation.is_some()
            || call.resume_fiber_continuation.is_some()
            || call.running_generator.is_some()
            || call.running_fiber.is_some()
        {
            return CachedDenseFunctionDispatch::Continue(call);
        }

        let plan = match self.get_or_build_dense_execution_plan(compiled) {
            Ok(plan) => plan,
            Err(message) => {
                let reason = dense_bytecode_unsupported_reason(&message);
                self.record_counter_bytecode_unsupported_reason(reason);
                if self.options.execution_format.is_strict_bytecode() {
                    return CachedDenseFunctionDispatch::Executed(Box::new(VmResult::unsupported(
                        output.clone(),
                        message,
                    )));
                }
                self.record_counter_bytecode_unsupported_fallback();
                self.record_counter_bytecode_auto_fallback_reason(reason);
                return CachedDenseFunctionDispatch::Continue(call);
            }
        };

        match plan.function_plan(function_id.index()) {
            Some(DenseFunctionPlan::Dense) => {
                let Some(dense_function) = plan.unit.functions.get(function_id.index()) else {
                    let message =
                        "E_PHP_VM_DENSE_BYTECODE_ENTRY: dense bytecode function is missing"
                            .to_string();
                    if self.options.execution_format.is_strict_bytecode() {
                        return CachedDenseFunctionDispatch::Executed(Box::new(
                            VmResult::unsupported(output.clone(), message),
                        ));
                    }
                    self.record_counter_bytecode_unsupported_reason(
                        dense_bytecode_unsupported_reason(&message),
                    );
                    return CachedDenseFunctionDispatch::Continue(call);
                };
                #[cfg(feature = "jit-cranelift")]
                if let Some(value) =
                    self.try_execute_dense_jit_leaf(compiled, state, function_id, function, &call)
                {
                    return CachedDenseFunctionDispatch::Executed(Box::new(
                        VmResult::success_no_output(Some(value)),
                    ));
                }
                CachedDenseFunctionDispatch::Executed(Box::new(self.execute_bytecode_function(
                    DenseExecutionRequest {
                        compiled,
                        dense: &plan.unit,
                        plan: Some(plan.as_ref()),
                        dense_function,
                        ir_function: function,
                        function_id,
                        call,
                        direct_call: None,
                        resume: None,
                    },
                    output,
                    stack,
                    state,
                )))
            }
            Some(DenseFunctionPlan::RichFallback { .. }) => {
                CachedDenseFunctionDispatch::Continue(call)
            }
            None => {
                let message =
                    "E_PHP_VM_DENSE_BYTECODE_ENTRY: dense execution plan entry is missing"
                        .to_string();
                if self.options.execution_format.is_strict_bytecode() {
                    CachedDenseFunctionDispatch::Executed(Box::new(VmResult::unsupported(
                        output.clone(),
                        message,
                    )))
                } else {
                    self.record_counter_bytecode_unsupported_reason(
                        dense_bytecode_unsupported_reason(&message),
                    );
                    CachedDenseFunctionDispatch::Continue(call)
                }
            }
        }
    }
}
