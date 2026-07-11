use super::*;

impl Vm {
    #[cfg(feature = "jit-cranelift")]
    pub(super) fn maybe_write_cranelift_clif_dump(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
    ) {
        let Some(path) = self.options.jit_dump_clif.as_ref() else {
            return;
        };
        let Ok(result) = php_jit::lower_function_to_cranelift(compiled.unit(), function_id) else {
            return;
        };
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && fs::create_dir_all(parent).is_err()
        {
            return;
        }
        let _ = fs::write(path, result.clif);
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_jit_compile_budget_spent(&self, compile_time_nanos: u64) {
        self.tiering
            .borrow_mut()
            .record_jit_compiled_function(compile_time_nanos);
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_jit_compile_failure_for_key(&self, key: JitFunctionKey) {
        if !self.options.jit_blacklist.enabled() {
            return;
        }
        let (blacklist_reason, invalidations) = {
            let mut jit = self.jit.borrow_mut();
            let entry = jit.functions.entry(key).or_default();
            let reason = entry.record_compile_error();
            let invalidations = if reason.is_some() {
                jit.invalidate_compile_cache_for_function(key.function)
            } else {
                0
            };
            (reason, invalidations)
        };
        self.record_counter_jit_compile_cache_invalidations(invalidations);
        if let Some(reason) = blacklist_reason {
            self.record_counter_jit_blacklisted_region(reason);
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_jit_side_exit_for_key(
        &self,
        key: JitFunctionKey,
        side_exit: php_jit::JitSideExit,
    ) {
        self.record_counter_jit_side_exit(side_exit.reason);
        let region_id = side_exit
            .resume_instruction
            .map(|instruction| format!("bytecode-{}", instruction.raw()))
            .unwrap_or_else(|| format!("function-{}", key.function.raw()));
        self.tiering.borrow_mut().record_jit_side_exit(
            key.function,
            region_id,
            side_exit.reason.as_str(),
            jit_guard_kind_for_side_exit(side_exit.reason),
        );
        if side_exit.reason == php_jit::SideExitReason::GuardFailed {
            self.record_counter_jit_guard_failure();
        }
        if !self.options.jit_blacklist.enabled() {
            return;
        }
        let (blacklist_reason, invalidations) = {
            let mut jit = self.jit.borrow_mut();
            let entry = jit.functions.entry(key).or_default();
            let reason = entry.record_side_exit(side_exit.reason);
            let invalidations = if reason.is_some() {
                jit.invalidate_compile_cache_for_function(key.function)
            } else {
                0
            };
            (reason, invalidations)
        };
        self.record_counter_jit_compile_cache_invalidations(invalidations);
        if let Some(reason) = blacklist_reason {
            self.record_counter_jit_blacklisted_region(reason);
        }
    }

    pub(super) fn record_inline_cache_site_event(
        &self,
        function_id: FunctionId,
        instruction_id: php_ir::ids::InstrId,
        observation: InlineCacheObservation,
    ) {
        self.record_counter_inline_cache_site(function_id, instruction_id.raw(), observation);
    }

    pub(super) fn observe_inline_cache(
        &self,
        unit_key: u64,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        kind: &InstructionKind,
    ) {
        if !self.options.tiering.enabled || !self.options.inline_caches.enabled() {
            return;
        }
        let Some(cache_kind) = crate::inline_cache::inline_cache_kind_for_instruction(kind) else {
            return;
        };
        let observation = self.inline_caches.borrow_mut().observe_slot(
            unit_key,
            function_id,
            block_id,
            instruction_id,
            cache_kind,
        );
        self.record_counter_inline_cache_site(function_id, instruction_id.raw(), observation);
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "property IC installation needs the resolved property metadata and callsite guard context"
    )]
    pub(super) fn maybe_install_property_fetch_inline_cache_target(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        property: &str,
        receiver_class: &str,
        receiver_entry: &ClassEntry,
        declaring_class: &ClassEntry,
        declaring_property: &ClassPropertyEntry,
        storage_name: &str,
        normalized_scope: Option<&str>,
        lookup_epoch: InvalidationEpoch,
        receiver_has_magic_get: bool,
        state: &ExecutionState,
        object: &ObjectRef,
        cache_id: Option<InlineCacheId>,
    ) {
        if !self.options.inline_caches.enabled()
            || declaring_property.flags.is_static
            || declaring_property.flags.is_protected
            || declaring_property.hooks.get.is_some()
            || declaring_property.hooks.set.is_some()
            || property_hook_is_active(state, object, declaring_class, declaring_property)
        {
            return;
        }
        let cache_scope = declaring_property
            .flags
            .is_private
            .then(|| normalize_class_name(&declaring_class.name));
        if declaring_property.flags.is_private && cache_scope.as_deref() != normalized_scope {
            return;
        }
        let layout = property_fetch_layout_metadata(
            receiver_entry,
            declaring_class,
            declaring_property,
            cache_scope.as_deref(),
            lookup_epoch,
            receiver_has_magic_get,
            false,
            false,
            true,
        );
        let target_payload = Arc::new(PropertyFetchResolvedTarget {
            receiver_class: receiver_class.to_owned(),
            declaring_class: declaring_class.name.clone(),
            property: declaring_property.name.clone(),
            storage_name: storage_name.to_owned(),
            layout,
            object_layout_epoch: object.class_layout_epoch(),
            declared_slot: object.declared_slot_index(storage_name),
        });
        let target = match dynamic_class_owner_index_in_state(state, &declaring_class.name) {
            Some(unit_index) => PropertyFetchCacheTarget::DynamicUnit {
                unit_index,
                target: target_payload,
            },
            None => PropertyFetchCacheTarget::CurrentUnit {
                target: target_payload,
            },
        };
        if let Some(id) = cache_id {
            self.inline_caches
                .borrow_mut()
                .install_property_fetch_by_id(
                    id,
                    property,
                    receiver_class,
                    cache_scope.as_deref(),
                    lookup_epoch,
                    target,
                );
        } else {
            self.install_property_fetch_inline_cache(
                compiled,
                function_id,
                block_id,
                instruction_id,
                property,
                receiver_class,
                cache_scope.as_deref(),
                lookup_epoch,
                target,
            );
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "property assignment IC installation needs resolved metadata and write guard context"
    )]
    pub(super) fn maybe_install_property_assign_inline_cache_target(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        property: &str,
        receiver_class: &str,
        receiver_entry: &ClassEntry,
        declaring_class: &ClassEntry,
        declaring_property: &ClassPropertyEntry,
        storage_name: &str,
        normalized_scope: Option<&str>,
        lookup_epoch: InvalidationEpoch,
        receiver_has_magic_set: bool,
        state: &ExecutionState,
        object: &ObjectRef,
        cache_id: Option<InlineCacheId>,
    ) {
        if !self.options.inline_caches.enabled() {
            return;
        }
        if declaring_property.flags.is_static || declaring_property.flags.is_protected {
            return;
        }
        if receiver_has_magic_set {
            return;
        }
        if declaring_property.flags.is_readonly || declaring_class.flags.is_readonly {
            return;
        }
        if declaring_property.hooks.get.is_some()
            || declaring_property.hooks.set.is_some()
            || property_hook_is_active(state, object, declaring_class, declaring_property)
        {
            return;
        }
        let cache_scope = declaring_property
            .flags
            .is_private
            .then(|| normalize_class_name(&declaring_class.name));
        if declaring_property.flags.is_private && cache_scope.as_deref() != normalized_scope {
            return;
        }
        if declaring_property.flags.set_is_private || declaring_property.flags.set_is_protected {
            return;
        }
        if matches!(object.get_property(storage_name), Some(Value::Reference(_))) {
            return;
        }
        let layout = property_assign_layout_metadata(
            receiver_entry,
            declaring_class,
            declaring_property,
            cache_scope.as_deref(),
            lookup_epoch,
            receiver_has_magic_set,
            false,
            false,
            false,
        );
        let target_payload = Arc::new(PropertyAssignResolvedTarget {
            receiver_class: receiver_class.to_owned(),
            declaring_class: declaring_class.name.clone(),
            property: declaring_property.name.clone(),
            storage_name: storage_name.to_owned(),
            layout,
            object_layout_epoch: object.class_layout_epoch(),
            declared_slot: object.declared_slot_index(storage_name),
            // Typed properties still need the per-write type check, so they
            // stay on the generic re-validation path. Readonly, hooks,
            // asymmetric set visibility, and references were rejected above.
            slot_write_eligible: declaring_property.type_.is_none(),
        });
        let target = match dynamic_class_owner_index_in_state(state, &declaring_class.name) {
            Some(unit_index) => PropertyAssignCacheTarget::DynamicUnit {
                unit_index,
                target: target_payload,
            },
            None => PropertyAssignCacheTarget::CurrentUnit {
                target: target_payload,
            },
        };
        if let Some(id) = cache_id {
            self.inline_caches
                .borrow_mut()
                .install_property_assign_by_id(
                    id,
                    property,
                    receiver_class,
                    cache_scope.as_deref(),
                    lookup_epoch,
                    target,
                );
        } else {
            self.install_property_assign_inline_cache(
                compiled,
                function_id,
                block_id,
                instruction_id,
                property,
                receiver_class,
                cache_scope.as_deref(),
                lookup_epoch,
                target,
            );
        }
    }

    pub(super) fn lookup_class_constant_static_property_inline_cache(
        &self,
        compiled: &CompiledUnit,
        cache_id: Option<InlineCacheId>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        kind: ClassConstantStaticPropertyCacheKind,
        resolved_class: &str,
        member: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> Option<ClassConstantStaticPropertyCacheTarget> {
        if !self.options.inline_caches.enabled() {
            return None;
        }
        let (target, observation) = if let Some(id) = cache_id {
            self.inline_caches
                .borrow_mut()
                .lookup_class_constant_static_property_by_id(
                    id,
                    kind,
                    resolved_class,
                    member,
                    scope,
                    epoch,
                )
        } else {
            self.inline_caches
                .borrow_mut()
                .lookup_class_constant_static_property(
                    compiled_unit_cache_key(compiled),
                    function_id,
                    block_id,
                    instruction_id,
                    kind,
                    resolved_class,
                    member,
                    scope,
                    epoch,
                )
        };
        self.record_inline_cache_site_event(function_id, instruction_id, observation);
        target
    }

    pub(super) fn install_class_constant_static_property_inline_cache(
        &self,
        compiled: &CompiledUnit,
        cache_id: Option<InlineCacheId>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        kind: ClassConstantStaticPropertyCacheKind,
        resolved_class: &str,
        member: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
        target: ClassConstantStaticPropertyCacheTarget,
    ) {
        if !self.options.inline_caches.enabled() {
            return;
        }
        if let Some(id) = cache_id {
            self.inline_caches
                .borrow_mut()
                .install_class_constant_static_property_by_id(
                    id,
                    kind,
                    resolved_class,
                    member,
                    scope,
                    epoch,
                    target,
                );
        } else {
            self.inline_caches
                .borrow_mut()
                .install_class_constant_static_property(
                    compiled_unit_cache_key(compiled),
                    function_id,
                    block_id,
                    instruction_id,
                    kind,
                    resolved_class,
                    member,
                    scope,
                    epoch,
                    target,
                );
        }
    }

    pub(super) fn observe_quickening(
        &self,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        kind: &InstructionKind,
    ) {
        if !self.options.tiering.enabled
            || !self.options.quickening.enabled()
            || self.adaptive_tiny_unit_setup_skipped.get()
            || !rich_quickening_candidate_kind(kind)
        {
            return;
        }
        let observation =
            self.quickening
                .borrow_mut()
                .observe(function_id, block_id, instruction_id);
        self.record_counter_quickening_site(function_id, instruction_id.raw(), observation);
    }

    fn record_quickening_guard(
        &self,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        hit: bool,
    ) {
        if !self.options.tiering.enabled
            || !self.options.quickening.enabled()
            || self.adaptive_tiny_unit_setup_skipped.get()
        {
            return;
        }
        let observation = self.quickening.borrow_mut().record_specialized_guard(
            function_id,
            block_id,
            instruction_id,
            hit,
        );
        self.record_counter_quickening_site(function_id, instruction_id.raw(), observation);
    }

    pub(super) fn observe_dense_quickening(
        &self,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
        opcode: DenseOpcode,
    ) {
        if !self.options.tiering.enabled
            || !self.options.quickening.enabled()
            || self.adaptive_tiny_unit_setup_skipped.get()
            || !dense_quickening_candidate_opcode(opcode)
        {
            return;
        }
        let observation =
            self.quickening
                .borrow_mut()
                .observe_dense(unit_id, function_id, instruction_index);
        self.record_counter_quickening_site(function_id, instruction_index, observation);
    }

    fn record_dense_quickening_guard(
        &self,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
        hit: bool,
    ) {
        if !self.options.tiering.enabled
            || !self.options.quickening.enabled()
            || self.adaptive_tiny_unit_setup_skipped.get()
        {
            return;
        }
        let observation = self.quickening.borrow_mut().record_dense_specialized_guard(
            unit_id,
            function_id,
            instruction_index,
            hit,
        );
        self.record_counter_quickening_site(function_id, instruction_index, observation);
    }

    /// Drops the transient shared array handle left in `register` by a
    /// dimension fetch whose block-local last use this was (Runtime lever R3).
    /// The plan already proved the register is dead after this fetch; only
    /// *shared* array handles are released, so dropping merely decrements the
    /// refcount — no contents are freed and no destructors run — while the
    /// array's owning local regains sole ownership and its next write mutates
    /// in place instead of copy-on-write-separating. Non-arrays, sole-owned
    /// arrays, and non-register operands are left untouched (byte-identical to
    /// the flag-off path).
    pub(super) fn release_dead_shared_array_register(&self, stack: &mut CallStack, register: u32) {
        let Some(frame) = stack.current_mut() else {
            return;
        };
        let reg = RegId::new(register);
        let is_shared_array = matches!(
            frame.registers.get(reg),
            Some(Value::Array(array)) if array.is_shared()
        );
        if !is_shared_array {
            return;
        }
        if let Ok(value) = frame.registers.take(reg) {
            drop(value);
            self.record_counter_last_use_array_read_release();
        }
    }

    /// Returns the memoized last-use move plan for a dense function, building it
    /// on first use. Returns `None` when the R3 flag is off, so callers keep the
    /// unchanged clone path.
    pub(super) fn last_use_move_plan(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        dense_function: &DenseFunction,
    ) -> Option<Rc<crate::last_use::LastUseMovePlan>> {
        if !self.options.last_use_moves {
            return None;
        }
        let key = (compiled_unit_cache_key(compiled), function_id.raw());
        if let Some(plan) = self.last_use_move_plans.borrow().get(&key) {
            return (!plan.is_empty()
                && (plan.move_checks_enabled() || plan.has_array_release_reads()))
            .then(|| Rc::clone(plan));
        }
        let plan = Rc::new(crate::last_use::LastUseMovePlan::analyze(dense_function));
        self.record_counter_last_use_move_ineligible(&plan);
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_last_use_plan_built(plan.eligible_reads(), plan.array_release_reads());
        }
        self.last_use_move_plans
            .borrow_mut()
            .insert(key, Rc::clone(&plan));
        (!plan.is_empty() && (plan.move_checks_enabled() || plan.has_array_release_reads()))
            .then_some(plan)
    }

    /// Reads a dense source operand, moving the value out of its register when
    /// the last-use plan marks this exact `(instruction, register)` read as a
    /// provably-safe last use. With `move_plan` `None` (R3 off) this is exactly
    /// `read_dense_operand` (a clone). Only register operands are ever moved;
    /// locals/constants take the clone path.
    pub(super) fn read_dense_operand_last_use(
        &self,
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        operand: DenseOperand,
        move_plan: Option<&crate::last_use::LastUseMovePlan>,
        dense_instruction_index: u32,
    ) -> Result<Value, String> {
        if let Some(plan) = move_plan
            && operand.kind == DenseOperandKind::Register
            && plan.move_checks_enabled()
        {
            if let Some(counters) = self.counters.borrow_mut().as_mut() {
                counters.record_last_use_move_consultation();
            }
            if plan.is_move_eligible(dense_instruction_index, operand.index) {
                let value = self.take_consumed_dense_operand(compiled, stack, operand)?;
                self.record_counter_last_use_move_applied(value_clone_is_heap(&value));
                return Ok(value);
            }
        }
        self.read_dense_operand(compiled, stack, operand)
    }

    /// Memoized: can this function's body observe its argument vector?
    pub(super) fn frame_args_elidable(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        function: &IrFunction,
    ) -> bool {
        !prepared_function_facts(compiled, function_id, function).observes_argument_vector
    }

    /// Returns the memoized frame-shape flags for a callee, scanning its body
    /// only on the first call to each (unit, function). Subsequent calls reuse
    /// the cached result instead of re-scanning the whole body per invocation.
    pub(super) fn frame_shape_flags(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        function: &IrFunction,
    ) -> FrameShapeFlags {
        let facts = prepared_function_facts(compiled, function_id, function);
        FrameShapeFlags {
            has_try_or_finally: facts.has_try_or_finally,
            may_hold_destructor_sensitive_value: facts.may_hold_destructor_sensitive_value,
            has_inline_blocker: facts.has_inline_blocker,
        }
    }

    /// Returns shared normalized/display handles for a class-name spelling,
    /// allocating them only on its first sighting. `with_class_context`
    /// re-derives both forms per call; the forms are pure functions of the
    /// spelling, so reusing the handles is behavior-neutral.
    pub(super) fn class_name_handles(&self, name: &str) -> ClassNameHandles {
        if let Some(handles) = self.class_name_handles.borrow().get(name) {
            return handles.clone();
        }
        let handles = ClassNameHandles {
            normalized: Arc::from(normalize_class_name(name)),
            display: Arc::from(display_class_name(name)),
        };
        self.class_name_handles
            .borrow_mut()
            .insert(name.to_owned(), handles.clone());
        handles
    }

    /// Returns the resolved runtime class entry for a class, building it only on
    /// the first instantiation within a class-table epoch and reusing the shared
    /// `Rc` afterward. When the class table changes (new class declared or
    /// autoloaded, tracked by `class_table_epoch`), the cache is dropped so
    /// lineage/property/constant resolution is recomputed against the new table.
    pub(super) fn cached_runtime_class_entry(
        &self,
        class_owner: &CompiledUnit,
        state: &ExecutionState,
        class: &php_ir::module::ClassEntry,
    ) -> Result<Rc<RuntimeClassEntry>, RuntimeClassEntryError> {
        let epoch = state.class_table_epoch;
        let key = normalize_class_name(&class.name);
        {
            let mut cache = self.runtime_class_entry_cache.borrow_mut();
            if cache.epoch != epoch {
                cache.entries.clear();
                cache.epoch = epoch;
            } else if let Some(entry) = cache.entries.get(&key) {
                return Ok(Rc::clone(entry));
            }
        }
        let entry = runtime_class_entry(
            class_owner,
            state,
            class,
            &|value| self.constant_value(class_owner.unit(), value),
            &|reference| class_constant_reference_value(class_owner, state, reference),
            &|reference| named_constant_reference_value(class_owner, state, reference),
        )?;
        let entry = Rc::new(entry);
        self.runtime_class_entry_cache
            .borrow_mut()
            .entries
            .insert(key, Rc::clone(&entry));
        Ok(entry)
    }

    /// Returns the raw IR class entry for a name, shared via `Rc`, cloning the
    /// (possibly large) class definition out of the class table only on the
    /// first `new` of each class within a class-table epoch. Subsequent
    /// instantiations reuse the shared `Rc` instead of re-resolving the name and
    /// deep-cloning the entry out of the `Arc` `lookup_class_in_state` returns.
    pub(super) fn cached_class_entry(
        &self,
        compiled: &CompiledUnit,
        state: &ExecutionState,
        class_name: &str,
    ) -> Option<Rc<php_ir::module::ClassEntry>> {
        let epoch = state.class_table_epoch;
        let key = normalize_class_name(class_name);
        {
            let mut cache = self.ir_class_entry_cache.borrow_mut();
            if cache.epoch != epoch {
                cache.entries.clear();
                cache.epoch = epoch;
            } else if let Some(entry) = cache.entries.get(&key) {
                return Some(Rc::clone(entry));
            }
        }
        let entry = Rc::new((*lookup_class_in_state(compiled, state, class_name)?).clone());
        self.ir_class_entry_cache
            .borrow_mut()
            .entries
            .insert(key, Rc::clone(&entry));
        Some(entry)
    }

    /// Returns the memoized default declared-slot template for a resolved
    /// runtime class, building it once per (class identity, class-table epoch)
    /// and reusing the shared `Rc` afterward. The hot `new C(...)` path clones
    /// the template into a fresh instance (see `ObjectRef::from_layout_slots`)
    /// instead of re-running the per-property default-materialization loop.
    ///
    /// The template is byte-identical to the `declared_slots`
    /// `ObjectRef::new_with_display_name` builds for the same class shape, and
    /// is independent of `display_name` (which only selects the debug-label
    /// layout variant). Keying by the class-table epoch means a redefinition
    /// (which bumps the epoch) rebuilds the template from the current entry, so
    /// stale defaults can never leak across a redeclaration.
    pub(super) fn cached_default_slot_template(
        &self,
        state: &ExecutionState,
        runtime_class: &RuntimeClassEntry,
        display_name: &str,
    ) -> Rc<Vec<Option<Value>>> {
        let epoch = state.class_table_epoch;
        let key = normalize_class_name(&runtime_class.name);
        {
            let mut cache = self.default_slot_template_cache.borrow_mut();
            if cache.epoch != epoch {
                cache.entries.clear();
                cache.epoch = epoch;
            } else if let Some(template) = cache.entries.get(&key) {
                return Rc::clone(template);
            }
        }
        let template = Rc::new(ObjectRef::default_declared_slots(
            runtime_class,
            display_name,
        ));
        self.default_slot_template_cache
            .borrow_mut()
            .entries
            .insert(key, Rc::clone(&template));
        template
    }

    /// Returns the resolved `__construct` for a class as seen from `caller_scope`,
    /// running the inheritance + visibility method-resolution walk only on the
    /// first `new` of each (class, caller scope) pair within a class-table epoch
    /// and reusing the memoized outcome afterward.
    ///
    /// The outcome is exactly what `lookup_resolved_method_in_state` returns for
    /// `"__construct"` — `Ok(Some(resolved))`, `Ok(None)` (no constructor → default
    /// construction), or `Err(message)` (e.g. an inheritance-cycle diagnostic) — so
    /// a cache hit reproduces the same result byte-for-byte, including errors. The
    /// caller scope is part of the key because private/protected resolution depends
    /// on it, and it is normalized (as `lookup_resolved_method_in_state` compares
    /// scopes case-insensitively) so equivalent scopes share one entry. When the
    /// class table changes (redeclaration or autoload, both bump
    /// `class_table_epoch`), the cache is dropped so resolution is recomputed
    /// against the new table.
    ///
    /// Downstream visibility enforcement (`validate_constructor_callable_in_state_scope`,
    /// abstract-class instantiation checks) runs on the returned `ResolvedMethodOwned`
    /// exactly as before and is not memoized here.
    pub(super) fn cached_constructor_resolution(
        &self,
        compiled: &CompiledUnit,
        state: &ExecutionState,
        class_name: &str,
        caller_scope: Option<&str>,
    ) -> Result<Option<ResolvedMethodOwned>, String> {
        let epoch = state.class_table_epoch;
        let key = (
            normalize_class_name(class_name),
            caller_scope.map(normalize_class_name),
        );
        {
            let mut cache = self.constructor_resolution_cache.borrow_mut();
            if cache.epoch != epoch {
                cache.entries.clear();
                cache.epoch = epoch;
            } else if let Some(outcome) = cache.entries.get(&key) {
                return outcome.clone();
            }
        }
        let outcome = lookup_resolved_method_in_state(
            compiled,
            state,
            class_name,
            "__construct",
            caller_scope,
        );
        self.constructor_resolution_cache
            .borrow_mut()
            .entries
            .insert(key, outcome.clone());
        outcome
    }

    fn record_quickened_concat_guard(
        &self,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        hit: bool,
    ) {
        self.record_quickening_guard(function_id, block_id, instruction_id, hit);
        self.record_counter_string_concat_fast_path(hit);
    }

    fn record_quickened_dense_concat_guard(
        &self,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
        hit: bool,
    ) {
        self.record_dense_quickening_guard(unit_id, function_id, instruction_index, hit);
        self.record_counter_string_concat_fast_path(hit);
    }

    pub(super) fn try_array_shape_lookup(
        &self,
        array: &PhpArray,
        key: &ArrayKey,
    ) -> Option<Option<Value>> {
        let metadata = array.shape_metadata();
        self.record_counter_array_shape_observed(array);
        if metadata.numeric_string_key_ambiguity {
            self.record_counter_array_shape_lookup_fallback(
                PhpArrayShapeLookupFallback::KeyCoercion,
            );
            return None;
        }
        match metadata.kind {
            PhpArrayShapeKind::InternedStringKeyRecord
            | PhpArrayShapeKind::ShapeStableRecordLike => {
                let lookup = array.record_shape_string_key_lookup(key);
                self.record_counter_record_shape_lookup(&lookup);
                match lookup {
                    PhpArrayShapeLookup::Hit(value) => Some(Some(effective_value(value))),
                    PhpArrayShapeLookup::Miss => Some(None),
                    PhpArrayShapeLookup::Fallback(_) => None,
                }
            }
            PhpArrayShapeKind::SmallInlineMap => {
                let lookup = array.small_map_lookup(key);
                self.record_counter_small_map_lookup(&lookup);
                match lookup {
                    PhpArrayShapeLookup::Hit(value) => Some(Some(effective_value(value))),
                    PhpArrayShapeLookup::Miss => Some(None),
                    PhpArrayShapeLookup::Fallback(_) => None,
                }
            }
            PhpArrayShapeKind::CowOrReferenceFallback => {
                self.record_counter_array_shape_lookup_fallback(
                    PhpArrayShapeLookupFallback::CowOrReference,
                );
                None
            }
            PhpArrayShapeKind::PackedWithHoles | PhpArrayShapeKind::MixedHash => {
                self.record_counter_array_shape_lookup_fallback(
                    PhpArrayShapeLookupFallback::OrderSemantics,
                );
                None
            }
            PhpArrayShapeKind::Empty
            | PhpArrayShapeKind::Packed
            | PhpArrayShapeKind::SharedImmutableLiteralArray => None,
        }
    }

    pub(super) fn record_array_count_fast_path_if_applicable(&self, name: &str, values: &[Value]) {
        if name != "count" {
            return;
        }
        let Some(first) = values.first() else {
            return;
        };
        let recursive_mode = values
            .get(1)
            .is_some_and(|value| matches!(effective_value(value), Value::Int(1)));
        if recursive_mode {
            return;
        }
        if matches!(effective_value(first), Value::Array(_)) {
            self.record_counter_array_count_fast_path_hit();
        }
    }

    pub(super) fn lookup_internal_function_dispatch(&self, name: &str) -> Option<BuiltinEntry> {
        if !self.options.internal_function_dispatch_cache {
            return BuiltinRegistry::new().get(name);
        }
        let (entry, outcome) = self
            .internal_function_dispatch_cache
            .borrow_mut()
            .lookup(name);
        match outcome {
            InternalFunctionDispatchCacheOutcome::Hit => {
                self.record_counter_internal_function_dispatch_cache(true);
            }
            InternalFunctionDispatchCacheOutcome::Miss => {
                self.record_counter_internal_function_dispatch_cache(false);
            }
            InternalFunctionDispatchCacheOutcome::Uncached => {}
        }
        entry
    }

    pub(super) fn typecheck_fast_path_context(&self) -> TypecheckFastPathContext<'_> {
        TypecheckFastPathContext::new(
            self.options.typecheck_fast_paths,
            self.options.collect_counters.then_some(&self.counters),
        )
    }

    fn record_quickened_packed_dim_guard(
        &self,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        hit: bool,
    ) {
        self.record_quickening_guard(function_id, block_id, instruction_id, hit);
        self.record_counter_packed_dim_fast_path(hit);
    }

    pub(super) fn try_quickened_int_int_binary(
        &self,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        op: BinaryOp,
        lhs: &Value,
        rhs: &Value,
    ) -> Option<Value> {
        if !self.options.quickening.enabled() {
            return None;
        }
        let expected = int_int_specialization_for_binary_op(op)?;

        let specialization = {
            self.quickening
                .borrow()
                .specialization(function_id, block_id, instruction_id)
        };

        match specialization {
            Some(specialization) if specialization == expected => match (lhs, rhs) {
                (Value::Int(lhs), Value::Int(rhs)) => {
                    if let Some(value) = checked_int_binary(op, *lhs, *rhs) {
                        self.record_quickening_guard(function_id, block_id, instruction_id, true);
                        Some(Value::Int(value))
                    } else {
                        self.record_quickening_guard(function_id, block_id, instruction_id, false);
                        None
                    }
                }
                _ => {
                    self.record_quickening_guard(function_id, block_id, instruction_id, false);
                    None
                }
            },
            None => {
                if matches!(lhs, Value::Int(_)) && matches!(rhs, Value::Int(_)) {
                    let observation = match op {
                        BinaryOp::Add => self
                            .quickening
                            .borrow_mut()
                            .observe_add_int_int_candidate(function_id, block_id, instruction_id),
                        BinaryOp::Sub => self
                            .quickening
                            .borrow_mut()
                            .observe_sub_int_int_candidate(function_id, block_id, instruction_id),
                        BinaryOp::Mul => self
                            .quickening
                            .borrow_mut()
                            .observe_mul_int_int_candidate(function_id, block_id, instruction_id),
                        _ => return None,
                    };
                    self.record_counter_quickening_site(
                        function_id,
                        instruction_id.raw(),
                        observation,
                    );
                }
                None
            }
            Some(_) => None,
        }
    }

    pub(super) fn try_quickened_concat_string_string(
        &self,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        lhs: &Value,
        rhs: &Value,
    ) -> Option<Value> {
        if !self.options.quickening.enabled() {
            return None;
        }

        let specialization = {
            self.quickening
                .borrow()
                .specialization(function_id, block_id, instruction_id)
        };

        match specialization {
            Some(QuickeningSpecialization::ConcatStringString) => match (lhs, rhs) {
                (Value::String(lhs), Value::String(rhs)) => {
                    if lhs.len().checked_add(rhs.len()).is_none() {
                        self.record_quickened_concat_guard(
                            function_id,
                            block_id,
                            instruction_id,
                            false,
                        );
                        return None;
                    }
                    self.record_quickened_concat_guard(function_id, block_id, instruction_id, true);
                    self.record_counter_concat_prealloc_hit();
                    Some(Value::String(PhpString::from_parts(&[
                        lhs.as_bytes(),
                        rhs.as_bytes(),
                    ])))
                }
                _ => {
                    self.record_quickened_concat_guard(
                        function_id,
                        block_id,
                        instruction_id,
                        false,
                    );
                    None
                }
            },
            None => {
                if matches!(lhs, Value::String(_)) && matches!(rhs, Value::String(_)) {
                    let observation = self
                        .quickening
                        .borrow_mut()
                        .observe_concat_string_string_candidate(
                            function_id,
                            block_id,
                            instruction_id,
                        );
                    self.record_counter_quickening_site(
                        function_id,
                        instruction_id.raw(),
                        observation,
                    );
                }
                None
            }
            Some(
                QuickeningSpecialization::AddIntInt
                | QuickeningSpecialization::SubIntInt
                | QuickeningSpecialization::MulIntInt
                | QuickeningSpecialization::PackedArrayIntKey
                | QuickeningSpecialization::BoolBranchCondition,
            ) => None,
        }
    }

    pub(super) fn try_quickened_packed_array_int_key(
        &self,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        array: &Value,
        key: &Value,
    ) -> Option<Value> {
        if !self.options.quickening.enabled() {
            return None;
        }

        let specialization = {
            self.quickening
                .borrow()
                .specialization(function_id, block_id, instruction_id)
        };

        match specialization {
            Some(QuickeningSpecialization::PackedArrayIntKey) => match (array, key) {
                (Value::Array(array), Value::Int(index)) if *index >= 0 => {
                    let metadata = array.packed_metadata();
                    if metadata.contains_references {
                        self.record_counter_dequickened_by_reference(
                            AliasState::PropertyOrArrayDimReference,
                        );
                        self.record_quickened_packed_dim_guard(
                            function_id,
                            block_id,
                            instruction_id,
                            false,
                        );
                        self.record_counter_cow_or_reference_fallback();
                        return None;
                    }
                    if metadata.kind != PhpArrayKind::PackedList {
                        self.record_quickened_packed_dim_guard(
                            function_id,
                            block_id,
                            instruction_id,
                            false,
                        );
                        if metadata.numeric_string_key_ambiguity {
                            self.record_counter_array_fast_path_fallback("numeric_string_key");
                        } else {
                            self.record_counter_packed_fetch_layout_exit();
                        }
                        return None;
                    }
                    if let Some(value) = array.packed_element_fast(*index as usize) {
                        self.record_quickened_packed_dim_guard(
                            function_id,
                            block_id,
                            instruction_id,
                            true,
                        );
                        self.record_counter_packed_fetch_fast_hit();
                        self.record_counter_array_packed_read_fast_path_hit();
                        Some(effective_value(value))
                    } else {
                        self.record_quickened_packed_dim_guard(
                            function_id,
                            block_id,
                            instruction_id,
                            false,
                        );
                        self.record_counter_packed_fetch_bounds_exit();
                        None
                    }
                }
                (Value::Array(array), _) => {
                    self.record_quickened_packed_dim_guard(
                        function_id,
                        block_id,
                        instruction_id,
                        false,
                    );
                    let metadata = array.packed_metadata();
                    if metadata.contains_references {
                        self.record_counter_dequickened_by_reference(
                            AliasState::PropertyOrArrayDimReference,
                        );
                        self.record_counter_cow_or_reference_fallback();
                    } else if metadata.numeric_string_key_ambiguity
                        || value_is_numeric_string_key_ambiguity(key)
                    {
                        self.record_counter_array_fast_path_fallback("numeric_string_key");
                    } else {
                        self.record_counter_packed_fetch_layout_exit();
                    }
                    None
                }
                _ => {
                    self.record_quickened_packed_dim_guard(
                        function_id,
                        block_id,
                        instruction_id,
                        false,
                    );
                    self.record_counter_packed_fetch_layout_exit();
                    None
                }
            },
            None => {
                if let (Value::Array(array), Value::Int(index)) = (array, key)
                    && *index >= 0
                    && !array.contains_references_fast()
                    && array.packed_element_fast(*index as usize).is_some()
                {
                    let observation = self
                        .quickening
                        .borrow_mut()
                        .observe_packed_array_int_key_candidate(
                            function_id,
                            block_id,
                            instruction_id,
                        );
                    self.record_counter_quickening_site(
                        function_id,
                        instruction_id.raw(),
                        observation,
                    );
                }
                None
            }
            Some(
                QuickeningSpecialization::AddIntInt
                | QuickeningSpecialization::SubIntInt
                | QuickeningSpecialization::MulIntInt
                | QuickeningSpecialization::ConcatStringString
                | QuickeningSpecialization::BoolBranchCondition,
            ) => None,
        }
    }

    pub(super) fn try_quickened_dense_int_int_binary(
        &self,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
        op: BinaryOp,
        lhs: &Value,
        rhs: &Value,
    ) -> Option<Value> {
        if !self.options.quickening.enabled() {
            return None;
        }
        let expected = int_int_specialization_for_binary_op(op)?;
        let specialization = {
            self.quickening
                .borrow()
                .dense_specialization(unit_id, function_id, instruction_index)
        };

        match specialization {
            Some(specialization) if specialization == expected => match (lhs, rhs) {
                (Value::Int(lhs), Value::Int(rhs)) => {
                    if let Some(value) = checked_int_binary(op, *lhs, *rhs) {
                        self.record_dense_quickening_guard(
                            unit_id,
                            function_id,
                            instruction_index,
                            true,
                        );
                        Some(Value::Int(value))
                    } else {
                        self.record_dense_quickening_guard(
                            unit_id,
                            function_id,
                            instruction_index,
                            false,
                        );
                        None
                    }
                }
                _ => {
                    self.record_dense_quickening_guard(
                        unit_id,
                        function_id,
                        instruction_index,
                        false,
                    );
                    None
                }
            },
            None => {
                if matches!(lhs, Value::Int(_)) && matches!(rhs, Value::Int(_)) {
                    let observation = self
                        .quickening
                        .borrow_mut()
                        .observe_dense_int_int_candidate(
                            unit_id,
                            function_id,
                            instruction_index,
                            expected,
                        );
                    self.record_counter_quickening_site(
                        function_id,
                        instruction_index,
                        observation,
                    );
                }
                None
            }
            Some(_) => None,
        }
    }

    pub(super) fn try_quickened_dense_concat_string_string(
        &self,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
        lhs: &Value,
        rhs: &Value,
    ) -> Option<Value> {
        if !self.options.quickening.enabled() {
            return None;
        }
        let specialization = {
            self.quickening
                .borrow()
                .dense_specialization(unit_id, function_id, instruction_index)
        };

        match specialization {
            Some(QuickeningSpecialization::ConcatStringString) => match (lhs, rhs) {
                (Value::String(lhs), Value::String(rhs)) => {
                    if lhs.len().checked_add(rhs.len()).is_none() {
                        self.record_quickened_dense_concat_guard(
                            unit_id,
                            function_id,
                            instruction_index,
                            false,
                        );
                        return None;
                    }
                    self.record_quickened_dense_concat_guard(
                        unit_id,
                        function_id,
                        instruction_index,
                        true,
                    );
                    self.record_counter_concat_prealloc_hit();
                    Some(Value::String(PhpString::from_parts(&[
                        lhs.as_bytes(),
                        rhs.as_bytes(),
                    ])))
                }
                _ => {
                    self.record_quickened_dense_concat_guard(
                        unit_id,
                        function_id,
                        instruction_index,
                        false,
                    );
                    None
                }
            },
            None => {
                if matches!(lhs, Value::String(_)) && matches!(rhs, Value::String(_)) {
                    let observation = self
                        .quickening
                        .borrow_mut()
                        .observe_dense_concat_string_string_candidate(
                            unit_id,
                            function_id,
                            instruction_index,
                        );
                    self.record_counter_quickening_site(
                        function_id,
                        instruction_index,
                        observation,
                    );
                }
                None
            }
            Some(_) => None,
        }
    }

    fn try_quickened_dense_bool_branch(
        &self,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
        value: &Value,
    ) -> Option<bool> {
        if !self.options.quickening.enabled() {
            return None;
        }
        let specialization = {
            self.quickening
                .borrow()
                .dense_specialization(unit_id, function_id, instruction_index)
        };

        match specialization {
            Some(QuickeningSpecialization::BoolBranchCondition) => match value {
                Value::Bool(value) => {
                    self.record_dense_quickening_guard(
                        unit_id,
                        function_id,
                        instruction_index,
                        true,
                    );
                    Some(*value)
                }
                _ => {
                    self.record_dense_quickening_guard(
                        unit_id,
                        function_id,
                        instruction_index,
                        false,
                    );
                    None
                }
            },
            None => {
                if matches!(value, Value::Bool(_)) {
                    let observation = self
                        .quickening
                        .borrow_mut()
                        .observe_dense_bool_branch_candidate(
                            unit_id,
                            function_id,
                            instruction_index,
                        );
                    self.record_counter_quickening_site(
                        function_id,
                        instruction_index,
                        observation,
                    );
                }
                None
            }
            Some(_) => None,
        }
    }

    fn dense_branch_truthy_from_value(
        &self,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
        value: &Value,
    ) -> Result<bool, String> {
        if let Some(value) =
            self.try_quickened_dense_bool_branch(unit_id, function_id, instruction_index, value)
        {
            Ok(value)
        } else {
            to_bool(value)
        }
    }

    pub(super) fn read_dense_operand_branch_truthy(
        &self,
        compiled: &CompiledUnit,
        stack: &CallStack,
        operand: DenseOperand,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
    ) -> Result<bool, String> {
        match operand.kind {
            DenseOperandKind::Register => {
                let frame = stack.current().ok_or("no active frame")?;
                let Some(value) = frame.registers.get(RegId::new(operand.index)) else {
                    return Err(format!("invalid register r{}", operand.index));
                };
                if value.is_uninitialized() {
                    return Err(format!("read uninitialized register r{}", operand.index));
                }
                self.dense_branch_truthy_from_value(unit_id, function_id, instruction_index, value)
            }
            DenseOperandKind::Local => {
                let frame = stack.current().ok_or("no active frame")?;
                let Some(slot) = frame.locals.get_slot(LocalId::new(operand.index)) else {
                    return Err(format!("invalid local local:{}", operand.index));
                };
                match slot {
                    Slot::Value(value) if value.is_uninitialized() => self
                        .dense_branch_truthy_from_value(
                            unit_id,
                            function_id,
                            instruction_index,
                            &Value::Null,
                        ),
                    Slot::Value(value) => self.dense_branch_truthy_from_value(
                        unit_id,
                        function_id,
                        instruction_index,
                        value,
                    ),
                    Slot::Reference(cell) => {
                        let value = cell.borrow();
                        self.dense_branch_truthy_from_value(
                            unit_id,
                            function_id,
                            instruction_index,
                            &value,
                        )
                    }
                }
            }
            DenseOperandKind::Constant => {
                let value = self.cached_constant_value(compiled, ConstId::new(operand.index))?;
                self.dense_branch_truthy_from_value(unit_id, function_id, instruction_index, &value)
            }
        }
    }

    fn intern_bytes(&self, bytes: &[u8]) -> PhpString {
        let interned = self.literal_pool.borrow_mut().intern_bytes(bytes);
        let hit = interned.hit;
        let value = interned.value;
        self.record_counter_literal_intern(hit);
        value
    }

    fn intern_str(&self, value: &str) -> PhpString {
        self.intern_bytes(value.as_bytes())
    }

    pub(super) fn warm_literal_pool(&self, unit: &IrUnit) {
        for constant in &unit.constants {
            match constant {
                IrConstant::String(value) => {
                    self.intern_str(value);
                }
                IrConstant::StringBytes(value) => {
                    self.intern_bytes(value);
                }
                _ => {}
            }
        }
        for function in &unit.functions {
            self.intern_str(&function.name);
            for local in &function.locals {
                self.intern_str(local);
            }
            for param in &function.params {
                self.intern_str(&param.name);
            }
            for capture in &function.captures {
                self.intern_str(&capture.name);
            }
        }
        for entry in &unit.function_table {
            self.intern_str(&entry.name);
        }
        for entry in &unit.constant_table {
            self.intern_str(&entry.name);
        }
        for class in &unit.classes {
            self.intern_str(&class.name);
            self.intern_str(&class.display_name);
            if let Some(parent) = &class.parent {
                self.intern_str(parent);
            }
            for interface in &class.interfaces {
                self.intern_str(interface);
            }
            for method in &class.methods {
                self.intern_str(&method.name);
                self.intern_str(&method.origin_class);
                for attribute in &method.attributes {
                    self.intern_attribute(attribute);
                }
            }
            for property in &class.properties {
                self.intern_str(&property.name);
                for attribute in &property.attributes {
                    self.intern_attribute(attribute);
                }
            }
            for constant in &class.constants {
                self.intern_str(&constant.name);
                for attribute in &constant.attributes {
                    self.intern_attribute(attribute);
                }
            }
            for case in &class.enum_cases {
                self.intern_str(&case.name);
                for attribute in &case.attributes {
                    self.intern_attribute(attribute);
                }
            }
            for attribute in &class.attributes {
                self.intern_attribute(attribute);
            }
        }
    }

    fn intern_attribute(&self, attribute: &php_ir::module::AttributeEntry) {
        self.intern_str(&attribute.name);
        if let Some(name) = &attribute.resolved_name {
            self.intern_str(name);
        }
        if let Some(name) = &attribute.fallback_name {
            self.intern_str(name);
        }
    }

    /// Resolves a materializable unit constant through the per-unit
    /// resolved-constant table: the first read interns/builds the value,
    /// every later read is an indexed refcount bump. Returns `None` for
    /// out-of-range ids and for constants that need per-read runtime
    /// resolution (named/class constants), so callers keep their exact
    /// existing error/fallback behavior for those.
    ///
    /// Sharing one value across reads is sound for the same reason the
    /// literal pool is: strings and arrays copy-on-write, so a mutation
    /// through any handle separates from the cached storage first.
    pub(super) fn resolved_constant_value(
        &self,
        compiled: &CompiledUnit,
        constant: ConstId,
    ) -> Option<Value> {
        let key = compiled.cache_identity();
        // Hit path: one borrow, an indexed cell read, one value clone —
        // no IR-table touch and no `Rc` traffic.
        let mut tables = self.resolved_constants.borrow_mut();
        if !matches!(&tables.last, Some((last_key, _)) if *last_key == key) {
            let table = Rc::clone(tables.tables.entry(key).or_insert_with(|| {
                std::iter::repeat_with(std::cell::OnceCell::new)
                    .take(compiled.unit().constants.len())
                    .collect()
            }));
            tables.last = Some((key, table));
        }
        let (_, table) = tables.last.as_ref()?;
        let cell = table.get(constant.index())?;
        if let Some(value) = cell.get() {
            return Some(value.clone());
        }
        let table = Rc::clone(table);
        drop(tables);
        // Miss path (first read of this id): named/class constants keep
        // their per-read runtime resolution and never populate the cell.
        let ir_constant = compiled.unit().constants.get(constant.index())?;
        if matches!(
            ir_constant,
            IrConstant::NamedConstant(_) | IrConstant::ClassConstant { .. }
        ) {
            return None;
        }
        let value = self.inline_constant_value(ir_constant);
        let _ = table.get(constant.index())?.set(value.clone());
        Some(value)
    }

    /// Table-backed variant of [`Self::constant_value`] for hot dense
    /// sites: falls through to the interning path (and its exact error
    /// and null-mapping behavior) whenever the table declines the id.
    pub(super) fn cached_constant_value(
        &self,
        compiled: &CompiledUnit,
        constant: ConstId,
    ) -> Result<Value, String> {
        if let Some(value) = self.resolved_constant_value(compiled, constant) {
            return Ok(value);
        }
        self.constant_value(compiled.unit(), constant)
    }

    pub(super) fn constant_value(&self, unit: &IrUnit, constant: ConstId) -> Result<Value, String> {
        let Some(value) = unit.constants.get(constant.index()) else {
            return Err(format!(
                "invalid constant const:{} for unit {} with {} constants",
                constant.raw(),
                unit.files
                    .first()
                    .map_or("<unknown>", |file| file.path.as_str()),
                unit.constants.len()
            ));
        };
        Ok(self.inline_constant_value(value))
    }

    pub(super) fn inline_constant_value(&self, constant: &IrConstant) -> Value {
        match constant {
            IrConstant::Null => Value::Null,
            IrConstant::Bool(value) => Value::Bool(*value),
            IrConstant::Int(value) => Value::Int(*value),
            IrConstant::Float(value) => Value::float(*value),
            IrConstant::String(value) => Value::String(self.intern_str(value)),
            IrConstant::StringBytes(value) => Value::String(self.intern_bytes(value)),
            IrConstant::NamedConstant(_) | IrConstant::ClassConstant { .. } => Value::Null,
            IrConstant::Array(entries) => {
                let mut array = PhpArray::new();
                for entry in entries {
                    let value = self.inline_constant_value(&entry.value);
                    if let Some(key) = &entry.key {
                        let key_value = self.inline_constant_value(key);
                        if let Some(key) = ArrayKey::from_value(&key_value) {
                            array.insert(key, value);
                        } else {
                            array.append(value);
                        }
                    } else {
                        array.append(value);
                    }
                }
                Value::Array(array)
            }
        }
    }

    pub(super) fn record_trace_event(
        &self,
        function_id: FunctionId,
        function: &IrFunction,
        stack: &mut CallStack,
        block_id: BlockId,
        instruction: &Instruction,
        output_len: usize,
    ) {
        let mut trace = self.trace.borrow_mut();
        let step = trace.len() + 1;
        trace.push(format!(
            "step={step} function={}({}) block={} instr={} kind={} stack_depth={} output_len={} locals=[{}] registers=[{}]",
            function.name,
            function_id.raw(),
            block_id.raw(),
            instruction.id.raw(),
            format_instruction_kind(&instruction.kind),
            stack.len(),
            output_len,
            format_locals(function, stack),
            format_registers(stack),
        ));
    }

    pub(super) fn record_lvalue_trace_event(
        &self,
        operation: &str,
        local: LocalId,
        dims: &[ArrayKey],
    ) {
        if !(self.options.trace || self.options.trace_runtime) {
            return;
        }
        let mut trace = self.trace.borrow_mut();
        let step = trace.len() + 1;
        trace.push(format!(
            "step={step} runtime lvalue operation={operation} local={} path=[{}]",
            local.raw(),
            dims.iter()
                .map(format_array_key_for_trace)
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }

    /// Lazily records one runtime trace line. The event closure only runs
    /// when tracing is enabled, so hot paths never pay for the string.
    pub(super) fn record_runtime_trace_event(&self, event: impl FnOnce() -> String) {
        if !self.options.trace_runtime {
            return;
        }
        let mut trace = self.trace.borrow_mut();
        let step = trace.len() + 1;
        let event = event();
        trace.push(format!("step={step} runtime {event}"));
    }

    pub(super) fn record_gc_root_trace_event(&self, stack: &CallStack, state: &ExecutionState) {
        if !self.options.trace_runtime {
            return;
        }
        let root_count = gc_root_count_from_vm_roots(stack, state);
        let snapshot = gc_snapshot_from_vm_roots(stack, state);
        self.record_runtime_trace_event(|| {
            format!(
                "gc-roots roots={} entities={} cycle_candidates={}",
                root_count,
                snapshot.nodes.len(),
                snapshot.cycle_candidates.len()
            )
        });
    }
}
