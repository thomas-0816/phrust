//! Read-only method-cache probes used before native call-target selection.

use super::*;

impl InlineCacheTable {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn peek_method_call_target(
        &self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        lowered_method: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> Option<MethodCallCacheTarget> {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::MethodCall,
        );
        let slot = self.slot(&key)?;
        Self::peek_method_call_target_in_slot(slot, lowered_method, receiver_class, scope, epoch)
    }

    #[must_use]
    pub fn peek_method_call_target_by_id(
        &self,
        id: InlineCacheId,
        lowered_method: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> Option<MethodCallCacheTarget> {
        let slot = self.slot_by_id(id)?;
        if slot.kind() != InlineCacheKind::MethodCall {
            return None;
        }
        Self::peek_method_call_target_in_slot(slot, lowered_method, receiver_class, scope, epoch)
    }

    fn peek_method_call_target_in_slot(
        slot: &InlineCacheSlot,
        lowered_method: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> Option<MethodCallCacheTarget> {
        if matches!(
            slot.state,
            InlineCacheState::Cold | InlineCacheState::Disabled | InlineCacheState::Megamorphic
        ) {
            return None;
        }
        slot.method_call_entries()
            .iter()
            .find(|entry| {
                entry.epoch == epoch
                    && method_guard_matches(
                        lowered_method,
                        &entry.lowered_method,
                        receiver_class,
                        &entry.receiver_class,
                        scope,
                        entry.scope.as_deref(),
                    )
            })
            .map(|entry| entry.target.clone())
    }
}
