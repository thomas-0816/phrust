//! Dense execution accessors for request-local inline-cache slots.

use super::prelude::*;

#[derive(Clone, Copy)]
pub(super) struct IrInlineCacheSite<'a> {
    pub(super) compiled: &'a CompiledUnit,
    pub(super) cache_id: Option<InlineCacheId>,
    pub(super) function: FunctionId,
    pub(super) block: BlockId,
    pub(super) instruction: InstrId,
}

impl<'a> IrInlineCacheSite<'a> {
    pub(super) fn classic(
        compiled: &'a CompiledUnit,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) -> Self {
        Self {
            compiled,
            cache_id: None,
            function,
            block,
            instruction,
        }
    }

    pub(super) fn hybrid(
        compiled: &'a CompiledUnit,
        cache_id: Option<InlineCacheId>,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) -> Self {
        Self {
            compiled,
            cache_id,
            function,
            block,
            instruction,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct DenseInlineCacheSite {
    pub(super) cache_id: InlineCacheId,
    pub(super) function: FunctionId,
    pub(super) instruction: InstrId,
}

impl DenseInlineCacheSite {
    pub(super) fn new(cache_id: InlineCacheId, function: FunctionId, instruction: InstrId) -> Self {
        Self {
            cache_id,
            function,
            instruction,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct UnitInlineCacheSite {
    pub(super) cache_id: Option<InlineCacheId>,
    pub(super) unit_key: u64,
    pub(super) function: FunctionId,
    pub(super) block: BlockId,
    pub(super) instruction: InstrId,
}

impl UnitInlineCacheSite {
    pub(super) fn new(
        cache_id: Option<InlineCacheId>,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) -> Self {
        Self {
            cache_id,
            unit_key,
            function,
            block,
            instruction,
        }
    }
}

impl Vm {
    pub(super) fn method_call_inline_cache_enabled(&self) -> bool {
        self.options.inline_caches.enabled()
            || matches!(
                self.options.native_optimization,
                NativeOptimizationPolicy::Optimizing
            )
    }

    pub(super) fn lookup_function_call_inline_cache(
        &self,
        site: IrInlineCacheSite<'_>,
        lowered_name: &PhpString,
        epoch: InvalidationEpoch,
        shape: &FunctionCallShape,
    ) -> Option<FunctionCallCacheTarget> {
        let IrInlineCacheSite {
            compiled,
            function,
            block,
            instruction,
            ..
        } = site;
        if !self.options.inline_caches.enabled() {
            return None;
        }
        let expected_builtin_metadata = self
            .inline_caches
            .borrow()
            .peek_function_call_builtin_metadata(
                compiled_unit_cache_key(compiled),
                function,
                block,
                instruction,
                lowered_name,
                shape,
            );
        let (target, observation) = self.inline_caches.borrow_mut().lookup_function_call(
            compiled_unit_cache_key(compiled),
            function,
            block,
            instruction,
            lowered_name,
            epoch,
            shape,
            expected_builtin_metadata.as_ref(),
        );
        self.record_inline_cache_site_event(function, instruction, observation);
        if observation.hit {
            self.record_counter_function_call_ic(true);
            if target.as_ref().is_some_and(function_call_target_is_builtin) {
                self.record_counter_builtin_call_ic(true);
            }
        } else if observation.miss {
            self.record_counter_function_call_ic(false);
            if observation.megamorphic {
                self.record_counter_call_ic_megamorphic_fallback();
            }
        }
        target
    }

    pub(super) fn install_function_call_inline_cache(
        &self,
        site: IrInlineCacheSite<'_>,
        lowered_name: &PhpString,
        epoch: InvalidationEpoch,
        shape: FunctionCallShape,
        target: FunctionCallCacheTarget,
    ) {
        let IrInlineCacheSite {
            compiled,
            function,
            block,
            instruction,
            ..
        } = site;
        if !self.options.inline_caches.enabled() {
            return;
        }
        let builtin_metadata = function_call_builtin_metadata(&target);
        self.inline_caches.borrow_mut().install_function_call(
            compiled_unit_cache_key(compiled),
            function,
            block,
            instruction,
            lowered_name,
            epoch,
            shape,
            builtin_metadata,
            target,
        );
    }

    pub(super) fn lookup_method_call_inline_cache(
        &self,
        site: IrInlineCacheSite<'_>,
        lowered_method: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> (
        Option<MethodCallCacheTarget>,
        Option<InlineCacheObservation>,
    ) {
        let IrInlineCacheSite {
            compiled,
            function,
            block,
            instruction,
            ..
        } = site;
        if !self.method_call_inline_cache_enabled() {
            return (None, None);
        }
        let (target, observation) = self.inline_caches.borrow_mut().lookup_method_call(
            compiled_unit_cache_key(compiled),
            function,
            block,
            instruction,
            lowered_method,
            receiver_class,
            scope,
            epoch,
        );
        self.record_inline_cache_site_event(function, instruction, observation);
        if observation.hit {
            self.record_counter_method_override_cache_hit();
        } else if observation.miss {
            self.record_counter_method_override_cache_miss();
        }
        (target, Some(observation))
    }

    pub(super) fn install_method_call_inline_cache(
        &self,
        site: IrInlineCacheSite<'_>,
        lowered_method: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
        target: MethodCallCacheTarget,
    ) {
        let IrInlineCacheSite {
            compiled,
            function,
            block,
            instruction,
            ..
        } = site;
        if !self.method_call_inline_cache_enabled() {
            return;
        }
        let mut inline_caches = self.inline_caches.borrow_mut();
        inline_caches.observe_slot(
            compiled_unit_cache_key(compiled),
            function,
            block,
            instruction,
            InlineCacheKind::MethodCall,
        );
        inline_caches.install_method_call(
            compiled_unit_cache_key(compiled),
            function,
            block,
            instruction,
            lowered_method,
            receiver_class,
            scope,
            epoch,
            target,
        );
    }

    pub(super) fn lookup_property_fetch_inline_cache(
        &self,
        site: IrInlineCacheSite<'_>,
        property: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> Option<PropertyFetchCacheTarget> {
        let IrInlineCacheSite {
            compiled,
            function,
            block,
            instruction,
            ..
        } = site;
        if !self.options.inline_caches.enabled() {
            return None;
        }
        let (target, observation) = self.inline_caches.borrow_mut().lookup_property_fetch(
            compiled_unit_cache_key(compiled),
            function,
            block,
            instruction,
            property,
            receiver_class,
            scope,
            epoch,
        );
        self.record_inline_cache_site_event(function, instruction, observation);
        target
    }

    pub(super) fn install_property_fetch_inline_cache(
        &self,
        site: IrInlineCacheSite<'_>,
        property: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
        target: PropertyFetchCacheTarget,
    ) {
        let IrInlineCacheSite {
            compiled,
            function,
            block,
            instruction,
            ..
        } = site;
        if !self.options.inline_caches.enabled() {
            return;
        }
        self.inline_caches.borrow_mut().install_property_fetch(
            compiled_unit_cache_key(compiled),
            function,
            block,
            instruction,
            property,
            receiver_class,
            scope,
            epoch,
            target,
        );
    }

    pub(super) fn lookup_property_assign_inline_cache(
        &self,
        site: IrInlineCacheSite<'_>,
        property: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> Option<PropertyAssignCacheTarget> {
        let IrInlineCacheSite {
            compiled,
            function,
            block,
            instruction,
            ..
        } = site;
        if !self.options.inline_caches.enabled() {
            return None;
        }
        let (target, observation) = self.inline_caches.borrow_mut().lookup_property_assign(
            compiled_unit_cache_key(compiled),
            function,
            block,
            instruction,
            property,
            receiver_class,
            scope,
            epoch,
        );
        self.record_inline_cache_site_event(function, instruction, observation);
        target
    }

    pub(super) fn install_property_assign_inline_cache(
        &self,
        site: IrInlineCacheSite<'_>,
        property: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
        target: PropertyAssignCacheTarget,
    ) {
        let IrInlineCacheSite {
            compiled,
            function,
            block,
            instruction,
            ..
        } = site;
        if !self.options.inline_caches.enabled() {
            return;
        }
        self.inline_caches.borrow_mut().install_property_assign(
            compiled_unit_cache_key(compiled),
            function,
            block,
            instruction,
            property,
            receiver_class,
            scope,
            epoch,
            target,
        );
    }

    pub(super) fn observe_dense_property_inline_cache(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        kind: InlineCacheKind,
    ) {
        if !self.options.inline_caches.enabled() {
            return;
        }
        let observation = self.inline_caches.borrow_mut().observe_slot(
            compiled_unit_cache_key(compiled),
            function_id,
            block_id,
            instruction_id,
            kind,
        );
        self.record_inline_cache_site_event(function_id, instruction_id, observation);
    }

    pub(super) fn observe_dense_call_inline_cache(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        kind: InlineCacheKind,
    ) {
        if !self.options.inline_caches.enabled() {
            return;
        }
        let observation = self.inline_caches.borrow_mut().observe_slot(
            compiled_unit_cache_key(compiled),
            function_id,
            block_id,
            instruction_id,
            kind,
        );
        self.record_inline_cache_site_event(function_id, instruction_id, observation);
    }

    pub(super) fn lookup_dense_function_call_inline_cache(
        &self,
        id: InlineCacheId,
        function_id: FunctionId,
        instruction_id: InstrId,
        lowered_name: &PhpString,
        epoch: InvalidationEpoch,
        shape: &FunctionCallShape,
    ) -> Option<FunctionCallCacheTarget> {
        let expected_builtin_metadata = self
            .inline_caches
            .borrow()
            .peek_function_call_builtin_metadata_by_id(id, lowered_name, shape);
        let (target, observation) = self.inline_caches.borrow_mut().lookup_function_call_by_id(
            id,
            lowered_name,
            epoch,
            shape,
            expected_builtin_metadata.as_ref(),
        );
        self.record_inline_cache_site_event(function_id, instruction_id, observation);
        if observation.hit {
            self.record_counter_function_call_ic(true);
            if target.as_ref().is_some_and(function_call_target_is_builtin) {
                self.record_counter_builtin_call_ic(true);
            }
        } else if observation.miss {
            self.record_counter_function_call_ic(false);
            if observation.megamorphic {
                self.record_counter_call_ic_megamorphic_fallback();
            }
        }
        target
    }

    pub(super) fn install_dense_function_call_inline_cache(
        &self,
        id: InlineCacheId,
        lowered_name: &PhpString,
        epoch: InvalidationEpoch,
        shape: FunctionCallShape,
        target: FunctionCallCacheTarget,
    ) {
        let builtin_metadata = function_call_builtin_metadata(&target);
        self.inline_caches.borrow_mut().install_function_call_by_id(
            id,
            lowered_name,
            epoch,
            shape,
            builtin_metadata,
            target,
        );
    }

    pub(super) fn lookup_dense_method_call_inline_cache(
        &self,
        site: DenseInlineCacheSite,
        lowered_method: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> (
        Option<MethodCallCacheTarget>,
        Option<InlineCacheObservation>,
    ) {
        let DenseInlineCacheSite {
            cache_id,
            function,
            instruction,
        } = site;
        let (target, observation) = self.inline_caches.borrow_mut().lookup_method_call_by_id(
            cache_id,
            lowered_method,
            receiver_class,
            scope,
            epoch,
        );
        self.record_inline_cache_site_event(function, instruction, observation);
        if observation.hit {
            self.record_counter_method_override_cache_hit();
        } else if observation.miss {
            self.record_counter_method_override_cache_miss();
        }
        (target, Some(observation))
    }

    pub(super) fn dense_method_call_inline_cache_has_target(
        &self,
        id: InlineCacheId,
        lowered_method: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> bool {
        self.inline_caches
            .borrow()
            .peek_method_call_target_by_id(id, lowered_method, receiver_class, scope, epoch)
            .is_some()
    }

    pub(super) fn method_call_inline_cache_has_target(
        &self,
        site: IrInlineCacheSite<'_>,
        lowered_method: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> bool {
        let IrInlineCacheSite {
            compiled,
            function,
            block,
            instruction,
            ..
        } = site;
        self.method_call_inline_cache_enabled()
            && self
                .inline_caches
                .borrow()
                .peek_method_call_target(
                    compiled_unit_cache_key(compiled),
                    function,
                    block,
                    instruction,
                    lowered_method,
                    receiver_class,
                    scope,
                    epoch,
                )
                .is_some()
    }

    pub(super) fn install_dense_method_call_inline_cache(
        &self,
        id: InlineCacheId,
        lowered_method: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
        target: MethodCallCacheTarget,
    ) {
        self.inline_caches.borrow_mut().install_method_call_by_id(
            id,
            lowered_method,
            receiver_class,
            scope,
            epoch,
            target,
        );
    }

    pub(super) fn lookup_dense_property_fetch_inline_cache(
        &self,
        site: DenseInlineCacheSite,
        property: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> Option<PropertyFetchCacheTarget> {
        let DenseInlineCacheSite {
            cache_id,
            function,
            instruction,
        } = site;
        let (target, observation) = self.inline_caches.borrow_mut().lookup_property_fetch_by_id(
            cache_id,
            property,
            receiver_class,
            scope,
            epoch,
        );
        self.record_inline_cache_site_event(function, instruction, observation);
        target
    }

    pub(super) fn lookup_dense_property_assign_inline_cache(
        &self,
        site: DenseInlineCacheSite,
        property: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> Option<PropertyAssignCacheTarget> {
        let DenseInlineCacheSite {
            cache_id,
            function,
            instruction,
        } = site;
        let (target, observation) = self
            .inline_caches
            .borrow_mut()
            .lookup_property_assign_by_id(cache_id, property, receiver_class, scope, epoch);
        self.record_inline_cache_site_event(function, instruction, observation);
        target
    }

    pub(super) fn lookup_include_path_inline_cache(
        &self,
        site: UnitInlineCacheSite,
        request: &IncludePathCacheKey,
        epoch: InvalidationEpoch,
    ) -> Option<IncludePathCacheTarget> {
        let UnitInlineCacheSite {
            cache_id,
            unit_key,
            function,
            block,
            instruction,
        } = site;
        if !self.options.inline_caches.enabled() {
            return None;
        }
        let (target, observation) = if let Some(cache_id) = cache_id {
            self.inline_caches
                .borrow_mut()
                .lookup_include_path_by_id(cache_id, request, epoch)
        } else {
            self.inline_caches.borrow_mut().lookup_include_path(
                unit_key,
                function,
                block,
                instruction,
                request,
                epoch,
            )
        };
        self.record_inline_cache_site_event(function, instruction, observation);
        target
    }

    pub(super) fn record_include_path_inline_cache_hit(
        &self,
        cache_id: Option<InlineCacheId>,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) {
        if !self.options.inline_caches.enabled() {
            return;
        }
        let observation = if let Some(cache_id) = cache_id {
            self.inline_caches
                .borrow_mut()
                .record_include_path_hit_by_id(cache_id)
        } else {
            self.inline_caches.borrow_mut().record_include_path_hit(
                unit_key,
                function,
                block,
                instruction,
            )
        };
        self.record_inline_cache_site_event(function, instruction, observation);
    }

    pub(super) fn record_include_path_inline_cache_invalidation(
        &self,
        cache_id: Option<InlineCacheId>,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) {
        if !self.options.inline_caches.enabled() {
            return;
        }
        let observation = if let Some(cache_id) = cache_id {
            self.inline_caches
                .borrow_mut()
                .invalidate_include_path_by_id(cache_id)
        } else {
            self.inline_caches.borrow_mut().invalidate_include_path(
                unit_key,
                function,
                block,
                instruction,
            )
        };
        self.record_inline_cache_site_event(function, instruction, observation);
    }

    pub(super) fn install_include_path_inline_cache(
        &self,
        site: UnitInlineCacheSite,
        request: IncludePathCacheKey,
        epoch: InvalidationEpoch,
        target: IncludePathCacheTarget,
    ) {
        let UnitInlineCacheSite {
            cache_id,
            unit_key,
            function,
            block,
            instruction,
        } = site;
        if !self.options.inline_caches.enabled() {
            return;
        }
        if let Some(cache_id) = cache_id {
            self.inline_caches
                .borrow_mut()
                .install_include_path_by_id(cache_id, request, epoch, target);
        } else {
            self.inline_caches.borrow_mut().install_include_path(
                unit_key,
                function,
                block,
                instruction,
                request,
                epoch,
                target,
            );
        }
    }

    pub(super) fn lookup_autoload_class_inline_cache(
        &self,
        site: UnitInlineCacheSite,
        request: &AutoloadClassLookupCacheKey,
        epochs: AutoloadClassLookupEpochs,
    ) -> Option<AutoloadClassLookupCacheTarget> {
        let UnitInlineCacheSite {
            cache_id,
            unit_key,
            function,
            block,
            instruction,
        } = site;
        if !self.options.inline_caches.enabled() {
            return None;
        }
        let (target, observation) = if let Some(cache_id) = cache_id {
            self.inline_caches
                .borrow_mut()
                .lookup_autoload_class_lookup_by_id(cache_id, request, epochs)
        } else {
            self.inline_caches
                .borrow_mut()
                .lookup_autoload_class_lookup(
                    unit_key,
                    function,
                    block,
                    instruction,
                    request,
                    epochs,
                )
        };
        self.record_inline_cache_site_event(function, instruction, observation);
        target
    }

    pub(super) fn observe_autoload_class_inline_cache(
        &self,
        cache_id: Option<InlineCacheId>,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) {
        if !self.options.inline_caches.enabled() {
            return;
        }
        if cache_id.is_none() {
            let observation = self.inline_caches.borrow_mut().observe_slot(
                unit_key,
                function,
                block,
                instruction,
                InlineCacheKind::AutoloadClassLookup,
            );
            self.record_inline_cache_site_event(function, instruction, observation);
        }
    }

    pub(super) fn invalidate_autoload_class_inline_cache(
        &self,
        cache_id: Option<InlineCacheId>,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) {
        if !self.options.inline_caches.enabled() {
            return;
        }
        let observation = if let Some(cache_id) = cache_id {
            self.inline_caches
                .borrow_mut()
                .invalidate_autoload_class_lookup_by_id(cache_id)
        } else {
            self.inline_caches
                .borrow_mut()
                .invalidate_autoload_class_lookup(unit_key, function, block, instruction)
        };
        self.record_inline_cache_site_event(function, instruction, observation);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn install_autoload_class_inline_cache(
        &self,
        cache_id: Option<InlineCacheId>,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        request: AutoloadClassLookupCacheKey,
        epochs: AutoloadClassLookupEpochs,
        target: AutoloadClassLookupCacheTarget,
    ) {
        if !self.options.inline_caches.enabled() {
            return;
        }
        if let Some(cache_id) = cache_id {
            self.inline_caches
                .borrow_mut()
                .install_autoload_class_lookup_by_id(cache_id, request, epochs, target);
        } else {
            self.inline_caches
                .borrow_mut()
                .install_autoload_class_lookup(
                    unit_key,
                    function,
                    block,
                    instruction,
                    request,
                    epochs,
                    target,
                );
        }
    }
}
