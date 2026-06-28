//! Request-local inline-cache side table for performance.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::include::IncludePathFileFingerprint;
use crate::{DEQUICKEN_AFTER_GUARD_MISSES, DISABLE_AFTER_GUARD_MISSES, FallbackProtocolStats};

use php_ir::{
    ids::{BlockId, FunctionId, InstrId},
    instruction::InstructionKind,
};

/// Small fixed guard-list size for experimental performance polymorphic method and
/// property inline caches.
pub const POLYMORPHIC_INLINE_CACHE_LIMIT: usize = 4;

/// Runtime inline-cache mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InlineCacheMode {
    /// Do not create or update inline-cache state.
    #[default]
    Off,
    /// Allocate request-local inline-cache slots without changing semantics.
    On,
}

impl InlineCacheMode {
    #[must_use]
    pub const fn enabled(self) -> bool {
        matches!(self, Self::On)
    }
}

/// Stable request-local inline-cache slot id.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InlineCacheId(u32);

impl InlineCacheId {
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Monotonic invalidation epoch carried by future IC guards.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InvalidationEpoch(u64);

impl InvalidationEpoch {
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Inline-cache family.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InlineCacheKind {
    FunctionCall,
    MethodCall,
    PropertyFetch,
    PropertyAssign,
    ClassConstantStaticProperty,
    IncludePath,
    AutoloadClassLookup,
    DimFetch,
}

impl InlineCacheKind {
    #[must_use]
    pub const fn counter_name(self) -> &'static str {
        match self {
            Self::FunctionCall => "function_call",
            Self::MethodCall => "method_call",
            Self::PropertyFetch => "property_fetch",
            Self::PropertyAssign => "property_assign",
            Self::ClassConstantStaticProperty => "class_constant_static_property",
            Self::IncludePath => "include_path",
            Self::AutoloadClassLookup => "autoload_class_lookup",
            Self::DimFetch => "dim_fetch",
        }
    }
}

/// Inline-cache lifecycle state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InlineCacheState {
    #[default]
    Cold,
    Monomorphic,
    Polymorphic,
    Megamorphic,
    Disabled,
}

/// Inline-cache stats for one slot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InlineCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub invalidations: u64,
    pub guard_failures: u64,
    pub megamorphic_transitions: u64,
    pub disabled_transitions: u64,
    pub protocol: FallbackProtocolStats,
}

/// VM-managed builtin groups that are resolved before generic user/internal
/// function lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FunctionCallBuiltinKind {
    AutoloadOrSymbolIntrospection,
    Config,
    ErrorHandling,
    OutputBuffering,
    Environment,
    Process,
    PcreCallback,
    ArrayCallback,
    ArraySort,
    InternalRegistry,
}

/// Resolution target cached by a function-call IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FunctionCallCacheTarget {
    CurrentUnit {
        function: FunctionId,
    },
    DynamicUnit {
        unit_index: usize,
        function: FunctionId,
    },
    Builtin {
        kind: FunctionCallBuiltinKind,
        name: String,
    },
}

/// Guarded argument metadata for a function-call IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionCallShape {
    pub arity: u32,
    pub named_arguments: Vec<String>,
    pub by_ref_arguments: Vec<bool>,
}

/// Guarded VM/runtime builtin implementation metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionCallBuiltinMetadata {
    pub implementation_id: String,
    pub version: u32,
}

/// One guarded function-call target in a polymorphic IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionCallPolymorphicEntry {
    pub lowered_name: String,
    pub epoch: InvalidationEpoch,
    pub shape: FunctionCallShape,
    pub builtin_metadata: Option<FunctionCallBuiltinMetadata>,
    pub target: FunctionCallCacheTarget,
}

/// Guarded argument metadata for a method-call IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodCallShape {
    pub arity: u32,
    pub named_arguments: Vec<String>,
    pub by_ref_arguments: Vec<bool>,
}

/// Stable method and receiver metadata guarded by a method-call IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodCallGuardMetadata {
    pub receiver_class_id: u32,
    pub class_layout_epoch: u64,
    pub method_table_epoch: u64,
    pub method_slot_index: Option<u32>,
    pub visibility_context: Option<String>,
    pub method_is_final: bool,
    pub method_is_private: bool,
    pub method_is_static: bool,
    pub receiver_has_override: bool,
    pub argument_shape: MethodCallShape,
    pub by_ref_compatible: bool,
    pub has_magic_call: bool,
}

/// Resolved method-call target payload kept out of interpreter stack frames.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodCallResolvedTarget {
    pub receiver_class: String,
    pub declaring_class: String,
    pub function: FunctionId,
    pub guard: MethodCallGuardMetadata,
}

/// Resolution target cached by a method-call IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MethodCallCacheTarget {
    CurrentUnit {
        target: Box<MethodCallResolvedTarget>,
    },
    DynamicUnit {
        unit_index: usize,
        target: Box<MethodCallResolvedTarget>,
    },
}

impl MethodCallCacheTarget {
    #[must_use]
    pub fn resolved_target(&self) -> &MethodCallResolvedTarget {
        match self {
            Self::CurrentUnit { target } | Self::DynamicUnit { target, .. } => target.as_ref(),
        }
    }

    #[must_use]
    pub fn receiver_class(&self) -> &str {
        &self.resolved_target().receiver_class
    }

    #[must_use]
    pub fn receiver_class_id(&self) -> u32 {
        self.resolved_target().guard.receiver_class_id
    }

    #[must_use]
    pub fn function(&self) -> FunctionId {
        self.resolved_target().function
    }
}

/// Stable layout metadata guarded by a property-fetch IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyFetchLayoutMetadata {
    pub class_id: u32,
    pub layout_version: u64,
    pub property_slot_index: Option<u32>,
    pub visibility_context: Option<String>,
    pub typed_property_initialized: bool,
    pub has_property_hooks: bool,
    pub has_magic_get: bool,
    pub dynamic_property_fallback: bool,
}

/// Stable layout and write-policy metadata guarded by a property-assignment IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyAssignLayoutMetadata {
    pub class_id: u32,
    pub layout_version: u64,
    pub property_slot_index: Option<u32>,
    pub visibility_context: Option<String>,
    pub typed_property: bool,
    pub readonly_or_init_only: bool,
    pub reference_slot: bool,
    pub has_property_hooks: bool,
    pub has_magic_set: bool,
    pub dynamic_property_fallback: bool,
}

/// Resolved property-fetch target payload kept out of interpreter stack frames.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyFetchResolvedTarget {
    pub receiver_class: String,
    pub declaring_class: String,
    pub property: String,
    pub storage_name: String,
    pub layout: PropertyFetchLayoutMetadata,
}

/// Resolution target cached by a property-fetch IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PropertyFetchCacheTarget {
    CurrentUnit {
        target: Box<PropertyFetchResolvedTarget>,
    },
    DynamicUnit {
        unit_index: usize,
        target: Box<PropertyFetchResolvedTarget>,
    },
}

impl PropertyFetchCacheTarget {
    #[must_use]
    pub fn resolved_target(&self) -> &PropertyFetchResolvedTarget {
        match self {
            Self::CurrentUnit { target } | Self::DynamicUnit { target, .. } => target.as_ref(),
        }
    }

    #[must_use]
    pub fn layout(&self) -> &PropertyFetchLayoutMetadata {
        &self.resolved_target().layout
    }

    #[must_use]
    pub fn receiver_class(&self) -> &str {
        &self.resolved_target().receiver_class
    }
}

/// Resolved property-assignment target payload kept out of interpreter stack frames.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyAssignResolvedTarget {
    pub receiver_class: String,
    pub declaring_class: String,
    pub property: String,
    pub storage_name: String,
    pub layout: PropertyAssignLayoutMetadata,
}

/// Resolution target cached by a property-assignment IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PropertyAssignCacheTarget {
    CurrentUnit {
        target: Box<PropertyAssignResolvedTarget>,
    },
    DynamicUnit {
        unit_index: usize,
        target: Box<PropertyAssignResolvedTarget>,
    },
}

impl PropertyAssignCacheTarget {
    #[must_use]
    pub fn resolved_target(&self) -> &PropertyAssignResolvedTarget {
        match self {
            Self::CurrentUnit { target } | Self::DynamicUnit { target, .. } => target.as_ref(),
        }
    }
}

/// One guarded method-call target in a polymorphic IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodCallPolymorphicEntry {
    pub lowered_method: String,
    pub receiver_class: String,
    pub scope: Option<String>,
    pub epoch: InvalidationEpoch,
    pub target: MethodCallCacheTarget,
}

/// One guarded property-fetch target in a polymorphic IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyFetchPolymorphicEntry {
    pub property: String,
    pub receiver_class: String,
    pub scope: Option<String>,
    pub epoch: InvalidationEpoch,
    pub target: PropertyFetchCacheTarget,
}

/// One guarded property-assignment target in a polymorphic IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyAssignPolymorphicEntry {
    pub property: String,
    pub receiver_class: String,
    pub scope: Option<String>,
    pub epoch: InvalidationEpoch,
    pub target: PropertyAssignCacheTarget,
}

/// Sub-kind cached in the shared class-constant/static-property IC family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassConstantStaticPropertyCacheKind {
    ClassConstant,
    EnumCase,
    StaticProperty,
}

/// Resolution target cached by a class-constant/static-property IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClassConstantStaticPropertyCacheTarget {
    CurrentUnit {
        kind: ClassConstantStaticPropertyCacheKind,
        resolved_class: String,
        declaring_class: String,
        member: String,
    },
    DynamicUnit {
        unit_index: usize,
        kind: ClassConstantStaticPropertyCacheKind,
        resolved_class: String,
        declaring_class: String,
        member: String,
    },
}

/// Request guards for include-/require-path resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncludePathCacheKey {
    pub path: String,
    pub include_path: Vec<PathBuf>,
    pub cwd: PathBuf,
    pub calling_file_directory: Option<PathBuf>,
}

/// Resolution target cached by an include-path IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncludePathCacheTarget {
    pub canonical_path: PathBuf,
    pub fingerprint: IncludePathFileFingerprint,
}

/// Class-like lookup flavor cached by autoload lookup IC slots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutoloadClassLookupKind {
    Class,
    Interface,
    Trait,
    Enum,
}

/// Stable request guard for class/interface/trait/enum lookup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutoloadClassLookupCacheKey {
    pub kind: AutoloadClassLookupKind,
    pub normalized_name: String,
    pub autoload_enabled: bool,
    pub autoload_stack_depth: usize,
    pub include_path_config: String,
    pub composer_map_fingerprint: Option<String>,
}

/// Epoch guards that make autoload lookup cache entries request-local and
/// invalidatable without changing lookup order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AutoloadClassLookupEpochs {
    pub autoload_stack_epoch: u64,
    pub class_table_epoch: u64,
    pub include_config_epoch: u64,
}

/// Cached result of a class-like lookup. Negative entries are installed only
/// for lookups that cannot suppress visible autoload side effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AutoloadClassLookupCacheTarget {
    Positive { display_name: String },
    Negative,
}

/// One request-local inline-cache slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InlineCacheSlot {
    pub id: InlineCacheId,
    pub kind: InlineCacheKind,
    pub state: InlineCacheState,
    pub unit_key: u64,
    pub function: FunctionId,
    pub block: BlockId,
    pub instruction: InstrId,
    pub epoch: InvalidationEpoch,
    pub stats: InlineCacheStats,
    pub function_call_name: Option<String>,
    pub function_call_shape: Option<FunctionCallShape>,
    pub function_call_builtin_metadata: Option<FunctionCallBuiltinMetadata>,
    pub function_call_target: Option<FunctionCallCacheTarget>,
    pub function_call_polymorphic_entries: Vec<FunctionCallPolymorphicEntry>,
    pub method_call_name: Option<String>,
    pub method_call_receiver_class: Option<String>,
    pub method_call_scope: Option<String>,
    pub method_call_target: Option<MethodCallCacheTarget>,
    pub method_call_polymorphic_entries: Vec<MethodCallPolymorphicEntry>,
    pub property_fetch_name: Option<String>,
    pub property_fetch_receiver_class: Option<String>,
    pub property_fetch_scope: Option<String>,
    pub property_fetch_target: Option<PropertyFetchCacheTarget>,
    pub property_fetch_polymorphic_entries: Vec<PropertyFetchPolymorphicEntry>,
    pub property_assign_name: Option<String>,
    pub property_assign_receiver_class: Option<String>,
    pub property_assign_scope: Option<String>,
    pub property_assign_target: Option<PropertyAssignCacheTarget>,
    pub property_assign_polymorphic_entries: Vec<PropertyAssignPolymorphicEntry>,
    pub class_static_kind: Option<ClassConstantStaticPropertyCacheKind>,
    pub class_static_resolved_class: Option<String>,
    pub class_static_member: Option<String>,
    pub class_static_scope: Option<String>,
    pub class_static_target: Option<ClassConstantStaticPropertyCacheTarget>,
    pub include_path_key: Option<IncludePathCacheKey>,
    pub include_path_target: Option<IncludePathCacheTarget>,
    pub autoload_class_lookup_key: Option<AutoloadClassLookupCacheKey>,
    pub autoload_class_lookup_epochs: Option<AutoloadClassLookupEpochs>,
    pub autoload_class_lookup_target: Option<AutoloadClassLookupCacheTarget>,
}

/// Result of observing one candidate instruction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InlineCacheObservation {
    pub candidate: bool,
    pub slot_allocated: bool,
    pub kind: Option<InlineCacheKind>,
    pub hit: bool,
    pub miss: bool,
    pub invalidation: bool,
    pub guard_failure: bool,
    pub fallback_call: bool,
    pub monomorphic: bool,
    pub polymorphic: bool,
    pub megamorphic: bool,
    pub disabled: bool,
}

impl InlineCacheObservation {
    #[must_use]
    pub const fn hit() -> Self {
        Self {
            hit: true,
            ..Self::empty()
        }
    }

    #[must_use]
    pub const fn miss() -> Self {
        Self {
            miss: true,
            fallback_call: true,
            ..Self::empty()
        }
    }

    #[must_use]
    pub const fn invalidation() -> Self {
        Self {
            miss: true,
            invalidation: true,
            fallback_call: true,
            ..Self::empty()
        }
    }

    #[must_use]
    pub const fn guard_failure() -> Self {
        Self {
            miss: true,
            guard_failure: true,
            fallback_call: true,
            ..Self::empty()
        }
    }

    #[must_use]
    pub const fn megamorphic() -> Self {
        Self {
            megamorphic: true,
            ..Self::empty()
        }
    }

    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            miss: true,
            fallback_call: true,
            disabled: true,
            ..Self::empty()
        }
    }

    #[must_use]
    pub const fn empty() -> Self {
        Self {
            candidate: false,
            slot_allocated: false,
            kind: None,
            hit: false,
            miss: false,
            invalidation: false,
            guard_failure: false,
            fallback_call: false,
            monomorphic: false,
            polymorphic: false,
            megamorphic: false,
            disabled: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct InlineCacheKey {
    unit_key: u64,
    function: u32,
    block: u32,
    instruction: u32,
    kind: InlineCacheKind,
}

fn with_kind(kind: InlineCacheKind, observation: InlineCacheObservation) -> InlineCacheObservation {
    InlineCacheObservation {
        kind: Some(kind),
        ..observation
    }
}

fn record_slot_hit(slot: &mut InlineCacheSlot) -> InlineCacheObservation {
    slot.stats.hits = slot.stats.hits.saturating_add(1);
    slot.stats.protocol.record_guard_hit();
    InlineCacheObservation {
        monomorphic: slot.state == InlineCacheState::Monomorphic,
        polymorphic: slot.state == InlineCacheState::Polymorphic,
        ..InlineCacheObservation::hit()
    }
}

fn record_slot_miss(slot: &mut InlineCacheSlot) -> InlineCacheObservation {
    slot.stats.misses = slot.stats.misses.saturating_add(1);
    slot.stats.protocol.record_cold_fallback();
    InlineCacheObservation::miss()
}

fn record_slot_invalidation(slot: &mut InlineCacheSlot) -> InlineCacheObservation {
    slot.stats.invalidations = slot.stats.invalidations.saturating_add(1);
    slot.stats.misses = slot.stats.misses.saturating_add(1);
    slot.stats.protocol.record_cold_fallback();
    slot.state = InlineCacheState::Cold;
    clear_slot_targets(slot);
    InlineCacheObservation::invalidation()
}

fn record_slot_guard_failure(slot: &mut InlineCacheSlot) -> InlineCacheObservation {
    slot.stats.guard_failures = slot.stats.guard_failures.saturating_add(1);
    slot.stats.misses = slot.stats.misses.saturating_add(1);
    let fallback = slot.stats.protocol.record_guard_fallback();
    let mut observation = InlineCacheObservation {
        guard_failure: fallback.guard_failure,
        fallback_call: fallback.fallback_call,
        miss: true,
        ..InlineCacheObservation::empty()
    };

    if slot.stats.guard_failures >= DISABLE_AFTER_GUARD_MISSES {
        let disabled = slot.stats.protocol.record_disabled();
        slot.stats.disabled_transitions = slot.stats.disabled_transitions.saturating_add(1);
        slot.state = InlineCacheState::Disabled;
        clear_slot_targets(slot);
        observation.disabled = disabled.disabled;
    } else if slot.stats.guard_failures >= DEQUICKEN_AFTER_GUARD_MISSES
        && slot.stats.megamorphic_transitions == 0
    {
        slot.stats.megamorphic_transitions = slot.stats.megamorphic_transitions.saturating_add(1);
        slot.stats.protocol.megamorphic_transitions = slot
            .stats
            .protocol
            .megamorphic_transitions
            .saturating_add(1);
        slot.state = InlineCacheState::Megamorphic;
        observation.megamorphic = true;
    }

    observation
}

fn disabled_slot_observation() -> InlineCacheObservation {
    InlineCacheObservation::miss()
}

fn megamorphic_slot_observation() -> InlineCacheObservation {
    InlineCacheObservation {
        miss: true,
        fallback_call: true,
        megamorphic: true,
        ..InlineCacheObservation::empty()
    }
}

fn mark_slot_megamorphic(slot: &mut InlineCacheSlot) {
    if slot.state == InlineCacheState::Megamorphic {
        return;
    }
    slot.stats.megamorphic_transitions = slot.stats.megamorphic_transitions.saturating_add(1);
    slot.stats.protocol.megamorphic_transitions = slot
        .stats
        .protocol
        .megamorphic_transitions
        .saturating_add(1);
    slot.state = InlineCacheState::Megamorphic;
    clear_slot_targets(slot);
}

fn clear_slot_targets(slot: &mut InlineCacheSlot) {
    slot.function_call_shape = None;
    slot.function_call_builtin_metadata = None;
    slot.function_call_target = None;
    slot.function_call_polymorphic_entries.clear();
    slot.method_call_target = None;
    slot.method_call_polymorphic_entries.clear();
    slot.property_fetch_target = None;
    slot.property_fetch_polymorphic_entries.clear();
    slot.property_assign_target = None;
    slot.property_assign_polymorphic_entries.clear();
    slot.class_static_target = None;
    slot.include_path_target = None;
    slot.autoload_class_lookup_target = None;
}

fn sync_function_call_primary(slot: &mut InlineCacheSlot) {
    let Some(first) = slot.function_call_polymorphic_entries.first() else {
        slot.state = InlineCacheState::Cold;
        slot.function_call_name = None;
        slot.function_call_shape = None;
        slot.function_call_builtin_metadata = None;
        slot.function_call_target = None;
        return;
    };
    slot.state = if slot.function_call_polymorphic_entries.len() == 1 {
        InlineCacheState::Monomorphic
    } else {
        InlineCacheState::Polymorphic
    };
    slot.epoch = first.epoch;
    slot.function_call_name = Some(first.lowered_name.clone());
    slot.function_call_shape = Some(first.shape.clone());
    slot.function_call_builtin_metadata = first.builtin_metadata.clone();
    slot.function_call_target = Some(first.target.clone());
}

fn method_guard_matches(
    lowered_method: &str,
    cached_method: &str,
    receiver_class: &str,
    cached_receiver_class: &str,
    scope: Option<&str>,
    cached_scope: Option<&str>,
) -> bool {
    cached_method == lowered_method
        && cached_receiver_class == receiver_class
        && cached_scope == scope
}

fn property_scope_matches(cached_scope: Option<&str>, scope: Option<&str>) -> bool {
    match cached_scope {
        Some(cached) => Some(cached) == scope,
        None => true,
    }
}

fn property_guard_matches(
    property: &str,
    cached_property: &str,
    receiver_class: &str,
    cached_receiver_class: &str,
    scope: Option<&str>,
    cached_scope: Option<&str>,
) -> bool {
    cached_property == property
        && cached_receiver_class == receiver_class
        && property_scope_matches(cached_scope, scope)
}

/// Per-request inline-cache metadata table.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InlineCacheTable {
    next_id: u32,
    slots: BTreeMap<InlineCacheKey, InlineCacheSlot>,
}

impl InlineCacheTable {
    /// Allocates or finds the slot for one inline-cache candidate.
    pub fn observe_slot(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        kind: InlineCacheKind,
    ) -> InlineCacheObservation {
        let key = inline_cache_key(unit_key, function, block, instruction, kind);
        if self.slots.contains_key(&key) {
            return InlineCacheObservation {
                candidate: true,
                kind: Some(kind),
                ..InlineCacheObservation::empty()
            };
        }

        let id = InlineCacheId::new(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.slots.insert(
            key,
            InlineCacheSlot {
                id,
                kind,
                state: InlineCacheState::Cold,
                unit_key,
                function,
                block,
                instruction,
                epoch: InvalidationEpoch::default(),
                stats: InlineCacheStats::default(),
                function_call_name: None,
                function_call_shape: None,
                function_call_builtin_metadata: None,
                function_call_target: None,
                function_call_polymorphic_entries: Vec::new(),
                method_call_name: None,
                method_call_receiver_class: None,
                method_call_scope: None,
                method_call_target: None,
                method_call_polymorphic_entries: Vec::new(),
                property_fetch_name: None,
                property_fetch_receiver_class: None,
                property_fetch_scope: None,
                property_fetch_target: None,
                property_fetch_polymorphic_entries: Vec::new(),
                property_assign_name: None,
                property_assign_receiver_class: None,
                property_assign_scope: None,
                property_assign_target: None,
                property_assign_polymorphic_entries: Vec::new(),
                class_static_kind: None,
                class_static_resolved_class: None,
                class_static_member: None,
                class_static_scope: None,
                class_static_target: None,
                include_path_key: None,
                include_path_target: None,
                autoload_class_lookup_key: None,
                autoload_class_lookup_epochs: None,
                autoload_class_lookup_target: None,
            },
        );
        InlineCacheObservation {
            candidate: true,
            slot_allocated: true,
            kind: Some(kind),
            ..InlineCacheObservation::empty()
        }
    }

    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    #[must_use]
    pub fn peek_function_call_target(
        &self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) -> Option<FunctionCallCacheTarget> {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::FunctionCall,
        );
        self.slots.get(&key)?.function_call_target.clone()
    }

    #[must_use]
    pub fn peek_function_call_builtin_metadata(
        &self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        lowered_name: &str,
        shape: &FunctionCallShape,
    ) -> Option<FunctionCallBuiltinMetadata> {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::FunctionCall,
        );
        let slot = self.slots.get(&key)?;
        slot.function_call_polymorphic_entries
            .iter()
            .find(|entry| entry.lowered_name == lowered_name && entry.shape == *shape)
            .and_then(|entry| entry.builtin_metadata.clone())
            .or_else(|| {
                (slot.function_call_name.as_deref() == Some(lowered_name)
                    && slot.function_call_shape.as_ref() == Some(shape))
                .then(|| slot.function_call_builtin_metadata.clone())
                .flatten()
            })
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "inline cache lookup APIs take the complete cache key and guard metadata explicitly"
    )]
    pub fn lookup_function_call(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        lowered_name: &str,
        epoch: InvalidationEpoch,
        shape: &FunctionCallShape,
        builtin_metadata: Option<&FunctionCallBuiltinMetadata>,
    ) -> (Option<FunctionCallCacheTarget>, InlineCacheObservation) {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::FunctionCall,
        );
        let Some(slot) = self.slots.get_mut(&key) else {
            return (
                None,
                with_kind(
                    InlineCacheKind::FunctionCall,
                    InlineCacheObservation::miss(),
                ),
            );
        };
        if slot.state == InlineCacheState::Disabled {
            return (
                None,
                with_kind(InlineCacheKind::FunctionCall, disabled_slot_observation()),
            );
        }
        if slot.state == InlineCacheState::Megamorphic {
            return (
                None,
                with_kind(
                    InlineCacheKind::FunctionCall,
                    megamorphic_slot_observation(),
                ),
            );
        }
        if !slot.function_call_polymorphic_entries.is_empty() {
            if let Some(index) = slot
                .function_call_polymorphic_entries
                .iter()
                .position(|entry| {
                    entry.lowered_name == lowered_name
                        && entry.shape == *shape
                        && entry.builtin_metadata.as_ref() == builtin_metadata
                })
            {
                let entry_epoch = slot.function_call_polymorphic_entries[index].epoch;
                if entry_epoch != epoch {
                    let observation = record_slot_invalidation(slot);
                    return (None, with_kind(InlineCacheKind::FunctionCall, observation));
                }
                let target = slot.function_call_polymorphic_entries[index].target.clone();
                let observation = record_slot_hit(slot);
                return (
                    Some(target),
                    with_kind(InlineCacheKind::FunctionCall, observation),
                );
            }
            let same_name = slot
                .function_call_polymorphic_entries
                .iter()
                .any(|entry| entry.lowered_name == lowered_name);
            let same_name_and_shape = slot
                .function_call_polymorphic_entries
                .iter()
                .any(|entry| entry.lowered_name == lowered_name && entry.shape == *shape);
            let observation = if same_name {
                record_slot_guard_failure(slot)
            } else if slot.function_call_polymorphic_entries.len() < POLYMORPHIC_INLINE_CACHE_LIMIT
            {
                record_slot_miss(slot)
            } else {
                let mut observation = record_slot_miss(slot);
                mark_slot_megamorphic(slot);
                observation.megamorphic = true;
                observation
            };
            if same_name_and_shape && !observation.guard_failure && !observation.megamorphic {
                let observation = record_slot_guard_failure(slot);
                return (None, with_kind(InlineCacheKind::FunctionCall, observation));
            }
            return (None, with_kind(InlineCacheKind::FunctionCall, observation));
        }
        let Some(cached_name) = slot.function_call_name.as_deref() else {
            let observation = record_slot_miss(slot);
            return (None, with_kind(InlineCacheKind::FunctionCall, observation));
        };
        if cached_name != lowered_name {
            let observation = record_slot_guard_failure(slot);
            return (None, with_kind(InlineCacheKind::FunctionCall, observation));
        }
        if slot.function_call_shape.as_ref() != Some(shape) {
            let observation = record_slot_guard_failure(slot);
            return (None, with_kind(InlineCacheKind::FunctionCall, observation));
        }
        if slot.function_call_builtin_metadata.as_ref() != builtin_metadata {
            let observation = record_slot_guard_failure(slot);
            return (None, with_kind(InlineCacheKind::FunctionCall, observation));
        }
        if slot.epoch != epoch {
            let observation = record_slot_invalidation(slot);
            return (None, with_kind(InlineCacheKind::FunctionCall, observation));
        }
        let Some(target) = slot.function_call_target.clone() else {
            let observation = record_slot_miss(slot);
            return (None, with_kind(InlineCacheKind::FunctionCall, observation));
        };
        let observation = record_slot_hit(slot);
        (
            Some(target),
            with_kind(InlineCacheKind::FunctionCall, observation),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "inline cache install APIs take the complete cache key and target metadata explicitly"
    )]
    pub fn install_function_call(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        lowered_name: &str,
        epoch: InvalidationEpoch,
        shape: FunctionCallShape,
        builtin_metadata: Option<FunctionCallBuiltinMetadata>,
        target: FunctionCallCacheTarget,
    ) {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::FunctionCall,
        );
        if let Some(slot) = self.slots.get_mut(&key) {
            if matches!(
                slot.state,
                InlineCacheState::Disabled | InlineCacheState::Megamorphic
            ) {
                return;
            }
            let new_entry = FunctionCallPolymorphicEntry {
                lowered_name: lowered_name.to_owned(),
                epoch,
                shape,
                builtin_metadata,
                target,
            };
            if let Some(index) = slot
                .function_call_polymorphic_entries
                .iter()
                .position(|entry| {
                    entry.lowered_name == new_entry.lowered_name
                        && entry.shape == new_entry.shape
                        && entry.builtin_metadata == new_entry.builtin_metadata
                })
            {
                slot.function_call_polymorphic_entries[index] = new_entry;
            } else {
                if slot.function_call_polymorphic_entries.len() >= POLYMORPHIC_INLINE_CACHE_LIMIT {
                    mark_slot_megamorphic(slot);
                    return;
                }
                slot.function_call_polymorphic_entries.push(new_entry);
            }
            sync_function_call_primary(slot);
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "inline cache lookup APIs take the complete cache key and guard metadata explicitly"
    )]
    pub fn lookup_method_call(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        lowered_method: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> (Option<MethodCallCacheTarget>, InlineCacheObservation) {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::MethodCall,
        );
        let Some(slot) = self.slots.get_mut(&key) else {
            return (
                None,
                with_kind(InlineCacheKind::MethodCall, InlineCacheObservation::miss()),
            );
        };
        if slot.state == InlineCacheState::Disabled {
            return (
                None,
                with_kind(InlineCacheKind::MethodCall, disabled_slot_observation()),
            );
        }
        if slot.state == InlineCacheState::Megamorphic {
            return (
                None,
                with_kind(InlineCacheKind::MethodCall, megamorphic_slot_observation()),
            );
        }
        if !slot.method_call_polymorphic_entries.is_empty() {
            if let Some(index) = slot
                .method_call_polymorphic_entries
                .iter()
                .position(|entry| {
                    method_guard_matches(
                        lowered_method,
                        &entry.lowered_method,
                        receiver_class,
                        &entry.receiver_class,
                        scope,
                        entry.scope.as_deref(),
                    )
                })
            {
                let entry_epoch = slot.method_call_polymorphic_entries[index].epoch;
                if entry_epoch != epoch {
                    let observation = record_slot_invalidation(slot);
                    return (None, with_kind(InlineCacheKind::MethodCall, observation));
                }
                let target = slot.method_call_polymorphic_entries[index].target.clone();
                let observation = record_slot_hit(slot);
                return (
                    Some(target),
                    with_kind(InlineCacheKind::MethodCall, observation),
                );
            }
            let same_method_and_scope = slot.method_call_polymorphic_entries.iter().any(|entry| {
                entry.lowered_method == lowered_method && entry.scope.as_deref() == scope
            });
            let observation = if same_method_and_scope
                && slot.method_call_polymorphic_entries.len() < POLYMORPHIC_INLINE_CACHE_LIMIT
            {
                record_slot_miss(slot)
            } else {
                record_slot_guard_failure(slot)
            };
            return (None, with_kind(InlineCacheKind::MethodCall, observation));
        }
        let Some(cached_method) = slot.method_call_name.as_deref() else {
            let observation = record_slot_miss(slot);
            return (None, with_kind(InlineCacheKind::MethodCall, observation));
        };
        let cached_receiver = slot.method_call_receiver_class.as_deref();
        let cached_scope = slot.method_call_scope.as_deref();
        if !method_guard_matches(
            lowered_method,
            cached_method,
            receiver_class,
            cached_receiver.unwrap_or_default(),
            scope,
            cached_scope,
        ) {
            let observation = record_slot_guard_failure(slot);
            return (None, with_kind(InlineCacheKind::MethodCall, observation));
        }
        if slot.epoch != epoch {
            let observation = record_slot_invalidation(slot);
            return (None, with_kind(InlineCacheKind::MethodCall, observation));
        }
        let Some(target) = slot.method_call_target.clone() else {
            let observation = record_slot_miss(slot);
            return (None, with_kind(InlineCacheKind::MethodCall, observation));
        };
        let observation = record_slot_hit(slot);
        (
            Some(target),
            with_kind(InlineCacheKind::MethodCall, observation),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "inline cache install APIs take the complete cache key and target metadata explicitly"
    )]
    pub fn install_method_call(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        lowered_method: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
        target: MethodCallCacheTarget,
    ) {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::MethodCall,
        );
        if let Some(slot) = self.slots.get_mut(&key) {
            if matches!(
                slot.state,
                InlineCacheState::Disabled | InlineCacheState::Megamorphic
            ) {
                return;
            }
            let new_entry = MethodCallPolymorphicEntry {
                lowered_method: lowered_method.to_owned(),
                receiver_class: receiver_class.to_owned(),
                scope: scope.map(str::to_owned),
                epoch,
                target,
            };
            if let Some(index) = slot
                .method_call_polymorphic_entries
                .iter()
                .position(|entry| {
                    method_guard_matches(
                        lowered_method,
                        &entry.lowered_method,
                        receiver_class,
                        &entry.receiver_class,
                        scope,
                        entry.scope.as_deref(),
                    )
                })
            {
                slot.method_call_polymorphic_entries[index] = new_entry;
                slot.state = InlineCacheState::Polymorphic;
                return;
            }
            if let (Some(cached_method), Some(cached_receiver), Some(cached_target)) = (
                slot.method_call_name.as_deref(),
                slot.method_call_receiver_class.as_deref(),
                slot.method_call_target.clone(),
            ) {
                if !method_guard_matches(
                    lowered_method,
                    cached_method,
                    receiver_class,
                    cached_receiver,
                    scope,
                    slot.method_call_scope.as_deref(),
                ) {
                    if slot.method_call_polymorphic_entries.is_empty() {
                        slot.method_call_polymorphic_entries
                            .push(MethodCallPolymorphicEntry {
                                lowered_method: cached_method.to_owned(),
                                receiver_class: cached_receiver.to_owned(),
                                scope: slot.method_call_scope.clone(),
                                epoch: slot.epoch,
                                target: cached_target,
                            });
                    }
                    if slot.method_call_polymorphic_entries.len() >= POLYMORPHIC_INLINE_CACHE_LIMIT
                    {
                        mark_slot_megamorphic(slot);
                        return;
                    }
                    slot.method_call_polymorphic_entries.push(new_entry);
                    slot.state = InlineCacheState::Polymorphic;
                    return;
                }
            } else if !slot.method_call_polymorphic_entries.is_empty() {
                if slot.method_call_polymorphic_entries.len() >= POLYMORPHIC_INLINE_CACHE_LIMIT {
                    mark_slot_megamorphic(slot);
                    return;
                }
                slot.method_call_polymorphic_entries.push(new_entry);
                slot.state = InlineCacheState::Polymorphic;
                return;
            }
            slot.state = InlineCacheState::Monomorphic;
            slot.epoch = epoch;
            slot.method_call_name = Some(lowered_method.to_owned());
            slot.method_call_receiver_class = Some(receiver_class.to_owned());
            slot.method_call_scope = scope.map(str::to_owned);
            slot.method_call_target = Some(new_entry.target);
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "inline cache lookup APIs take the complete cache key and guard metadata explicitly"
    )]
    pub fn lookup_property_fetch(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        property: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> (Option<PropertyFetchCacheTarget>, InlineCacheObservation) {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::PropertyFetch,
        );
        let Some(slot) = self.slots.get_mut(&key) else {
            return (
                None,
                with_kind(
                    InlineCacheKind::PropertyFetch,
                    InlineCacheObservation::miss(),
                ),
            );
        };
        if slot.state == InlineCacheState::Disabled {
            return (
                None,
                with_kind(InlineCacheKind::PropertyFetch, disabled_slot_observation()),
            );
        }
        if slot.state == InlineCacheState::Megamorphic {
            return (
                None,
                with_kind(
                    InlineCacheKind::PropertyFetch,
                    megamorphic_slot_observation(),
                ),
            );
        }
        if !slot.property_fetch_polymorphic_entries.is_empty() {
            if let Some(index) = slot
                .property_fetch_polymorphic_entries
                .iter()
                .position(|entry| {
                    property_guard_matches(
                        property,
                        &entry.property,
                        receiver_class,
                        &entry.receiver_class,
                        scope,
                        entry.scope.as_deref(),
                    )
                })
            {
                let entry_epoch = slot.property_fetch_polymorphic_entries[index].epoch;
                if entry_epoch != epoch {
                    let observation = record_slot_invalidation(slot);
                    return (None, with_kind(InlineCacheKind::PropertyFetch, observation));
                }
                let target = slot.property_fetch_polymorphic_entries[index]
                    .target
                    .clone();
                let observation = record_slot_hit(slot);
                return (
                    Some(target),
                    with_kind(InlineCacheKind::PropertyFetch, observation),
                );
            }
            let same_property_and_scope =
                slot.property_fetch_polymorphic_entries.iter().any(|entry| {
                    entry.property == property
                        && property_scope_matches(entry.scope.as_deref(), scope)
                });
            let observation = if same_property_and_scope
                && slot.property_fetch_polymorphic_entries.len() < POLYMORPHIC_INLINE_CACHE_LIMIT
            {
                record_slot_miss(slot)
            } else {
                record_slot_guard_failure(slot)
            };
            return (None, with_kind(InlineCacheKind::PropertyFetch, observation));
        }
        let Some(cached_property) = slot.property_fetch_name.as_deref() else {
            let observation = record_slot_miss(slot);
            return (None, with_kind(InlineCacheKind::PropertyFetch, observation));
        };
        let cached_receiver = slot.property_fetch_receiver_class.as_deref();
        let cached_scope = slot.property_fetch_scope.as_deref();
        if !property_guard_matches(
            property,
            cached_property,
            receiver_class,
            cached_receiver.unwrap_or_default(),
            scope,
            cached_scope,
        ) {
            let observation = record_slot_guard_failure(slot);
            return (None, with_kind(InlineCacheKind::PropertyFetch, observation));
        }
        if slot.epoch != epoch {
            let observation = record_slot_invalidation(slot);
            return (None, with_kind(InlineCacheKind::PropertyFetch, observation));
        }
        let Some(target) = slot.property_fetch_target.clone() else {
            let observation = record_slot_miss(slot);
            return (None, with_kind(InlineCacheKind::PropertyFetch, observation));
        };
        let observation = record_slot_hit(slot);
        (
            Some(target),
            with_kind(InlineCacheKind::PropertyFetch, observation),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "inline cache install APIs take the complete cache key and target metadata explicitly"
    )]
    pub fn install_property_fetch(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        property: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
        target: PropertyFetchCacheTarget,
    ) {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::PropertyFetch,
        );
        if let Some(slot) = self.slots.get_mut(&key) {
            if matches!(
                slot.state,
                InlineCacheState::Disabled | InlineCacheState::Megamorphic
            ) {
                return;
            }
            let new_entry = PropertyFetchPolymorphicEntry {
                property: property.to_owned(),
                receiver_class: receiver_class.to_owned(),
                scope: scope.map(str::to_owned),
                epoch,
                target,
            };
            if let Some(index) = slot
                .property_fetch_polymorphic_entries
                .iter()
                .position(|entry| {
                    property_guard_matches(
                        property,
                        &entry.property,
                        receiver_class,
                        &entry.receiver_class,
                        scope,
                        entry.scope.as_deref(),
                    )
                })
            {
                slot.property_fetch_polymorphic_entries[index] = new_entry;
                slot.state = InlineCacheState::Polymorphic;
                return;
            }
            if let (Some(cached_property), Some(cached_receiver), Some(cached_target)) = (
                slot.property_fetch_name.as_deref(),
                slot.property_fetch_receiver_class.as_deref(),
                slot.property_fetch_target.clone(),
            ) {
                if !property_guard_matches(
                    property,
                    cached_property,
                    receiver_class,
                    cached_receiver,
                    scope,
                    slot.property_fetch_scope.as_deref(),
                ) {
                    if slot.property_fetch_polymorphic_entries.is_empty() {
                        slot.property_fetch_polymorphic_entries.push(
                            PropertyFetchPolymorphicEntry {
                                property: cached_property.to_owned(),
                                receiver_class: cached_receiver.to_owned(),
                                scope: slot.property_fetch_scope.clone(),
                                epoch: slot.epoch,
                                target: cached_target,
                            },
                        );
                    }
                    if slot.property_fetch_polymorphic_entries.len()
                        >= POLYMORPHIC_INLINE_CACHE_LIMIT
                    {
                        mark_slot_megamorphic(slot);
                        return;
                    }
                    slot.property_fetch_polymorphic_entries.push(new_entry);
                    slot.state = InlineCacheState::Polymorphic;
                    return;
                }
            } else if !slot.property_fetch_polymorphic_entries.is_empty() {
                if slot.property_fetch_polymorphic_entries.len() >= POLYMORPHIC_INLINE_CACHE_LIMIT {
                    mark_slot_megamorphic(slot);
                    return;
                }
                slot.property_fetch_polymorphic_entries.push(new_entry);
                slot.state = InlineCacheState::Polymorphic;
                return;
            }
            slot.state = InlineCacheState::Monomorphic;
            slot.epoch = epoch;
            slot.property_fetch_name = Some(property.to_owned());
            slot.property_fetch_receiver_class = Some(receiver_class.to_owned());
            slot.property_fetch_scope = scope.map(str::to_owned);
            slot.property_fetch_target = Some(new_entry.target);
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "inline cache lookup APIs take the complete cache key and guard metadata explicitly"
    )]
    pub fn lookup_property_assign(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        property: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> (Option<PropertyAssignCacheTarget>, InlineCacheObservation) {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::PropertyAssign,
        );
        let Some(slot) = self.slots.get_mut(&key) else {
            return (
                None,
                with_kind(
                    InlineCacheKind::PropertyAssign,
                    InlineCacheObservation::miss(),
                ),
            );
        };
        if slot.state == InlineCacheState::Disabled {
            return (
                None,
                with_kind(InlineCacheKind::PropertyAssign, disabled_slot_observation()),
            );
        }
        if slot.state == InlineCacheState::Megamorphic {
            return (
                None,
                with_kind(
                    InlineCacheKind::PropertyAssign,
                    megamorphic_slot_observation(),
                ),
            );
        }
        if !slot.property_assign_polymorphic_entries.is_empty() {
            if let Some(index) = slot
                .property_assign_polymorphic_entries
                .iter()
                .position(|entry| {
                    property_guard_matches(
                        property,
                        &entry.property,
                        receiver_class,
                        &entry.receiver_class,
                        scope,
                        entry.scope.as_deref(),
                    )
                })
            {
                let entry_epoch = slot.property_assign_polymorphic_entries[index].epoch;
                if entry_epoch != epoch {
                    let observation = record_slot_invalidation(slot);
                    return (
                        None,
                        with_kind(InlineCacheKind::PropertyAssign, observation),
                    );
                }
                let target = slot.property_assign_polymorphic_entries[index]
                    .target
                    .clone();
                let observation = record_slot_hit(slot);
                return (
                    Some(target),
                    with_kind(InlineCacheKind::PropertyAssign, observation),
                );
            }
            let same_property_and_scope =
                slot.property_assign_polymorphic_entries
                    .iter()
                    .any(|entry| {
                        entry.property == property
                            && property_scope_matches(entry.scope.as_deref(), scope)
                    });
            let observation = if same_property_and_scope
                && slot.property_assign_polymorphic_entries.len() < POLYMORPHIC_INLINE_CACHE_LIMIT
            {
                record_slot_miss(slot)
            } else {
                record_slot_guard_failure(slot)
            };
            return (
                None,
                with_kind(InlineCacheKind::PropertyAssign, observation),
            );
        }
        let Some(cached_property) = slot.property_assign_name.as_deref() else {
            let observation = record_slot_miss(slot);
            return (
                None,
                with_kind(InlineCacheKind::PropertyAssign, observation),
            );
        };
        let cached_receiver = slot.property_assign_receiver_class.as_deref();
        let cached_scope = slot.property_assign_scope.as_deref();
        if !property_guard_matches(
            property,
            cached_property,
            receiver_class,
            cached_receiver.unwrap_or_default(),
            scope,
            cached_scope,
        ) {
            let observation = record_slot_guard_failure(slot);
            return (
                None,
                with_kind(InlineCacheKind::PropertyAssign, observation),
            );
        }
        if slot.epoch != epoch {
            let observation = record_slot_invalidation(slot);
            return (
                None,
                with_kind(InlineCacheKind::PropertyAssign, observation),
            );
        }
        let Some(target) = slot.property_assign_target.clone() else {
            let observation = record_slot_miss(slot);
            return (
                None,
                with_kind(InlineCacheKind::PropertyAssign, observation),
            );
        };
        let observation = record_slot_hit(slot);
        (
            Some(target),
            with_kind(InlineCacheKind::PropertyAssign, observation),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "inline cache install APIs take the complete cache key and target metadata explicitly"
    )]
    pub fn install_property_assign(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        property: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
        target: PropertyAssignCacheTarget,
    ) {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::PropertyAssign,
        );
        if let Some(slot) = self.slots.get_mut(&key) {
            if matches!(
                slot.state,
                InlineCacheState::Disabled | InlineCacheState::Megamorphic
            ) {
                return;
            }
            let new_entry = PropertyAssignPolymorphicEntry {
                property: property.to_owned(),
                receiver_class: receiver_class.to_owned(),
                scope: scope.map(str::to_owned),
                epoch,
                target,
            };
            if let Some(index) = slot
                .property_assign_polymorphic_entries
                .iter()
                .position(|entry| {
                    property_guard_matches(
                        property,
                        &entry.property,
                        receiver_class,
                        &entry.receiver_class,
                        scope,
                        entry.scope.as_deref(),
                    )
                })
            {
                slot.property_assign_polymorphic_entries[index] = new_entry;
                slot.state = InlineCacheState::Polymorphic;
                return;
            }
            if let (Some(cached_property), Some(cached_receiver), Some(cached_target)) = (
                slot.property_assign_name.as_deref(),
                slot.property_assign_receiver_class.as_deref(),
                slot.property_assign_target.clone(),
            ) {
                if !property_guard_matches(
                    property,
                    cached_property,
                    receiver_class,
                    cached_receiver,
                    scope,
                    slot.property_assign_scope.as_deref(),
                ) {
                    if slot.property_assign_polymorphic_entries.is_empty() {
                        slot.property_assign_polymorphic_entries.push(
                            PropertyAssignPolymorphicEntry {
                                property: cached_property.to_owned(),
                                receiver_class: cached_receiver.to_owned(),
                                scope: slot.property_assign_scope.clone(),
                                epoch: slot.epoch,
                                target: cached_target,
                            },
                        );
                    }
                    if slot.property_assign_polymorphic_entries.len()
                        >= POLYMORPHIC_INLINE_CACHE_LIMIT
                    {
                        mark_slot_megamorphic(slot);
                        return;
                    }
                    slot.property_assign_polymorphic_entries.push(new_entry);
                    slot.state = InlineCacheState::Polymorphic;
                    return;
                }
            } else if !slot.property_assign_polymorphic_entries.is_empty() {
                if slot.property_assign_polymorphic_entries.len() >= POLYMORPHIC_INLINE_CACHE_LIMIT
                {
                    mark_slot_megamorphic(slot);
                    return;
                }
                slot.property_assign_polymorphic_entries.push(new_entry);
                slot.state = InlineCacheState::Polymorphic;
                return;
            }
            slot.state = InlineCacheState::Monomorphic;
            slot.epoch = epoch;
            slot.property_assign_name = Some(property.to_owned());
            slot.property_assign_receiver_class = Some(receiver_class.to_owned());
            slot.property_assign_scope = scope.map(str::to_owned);
            slot.property_assign_target = Some(new_entry.target);
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "inline cache lookup APIs take the complete cache key and guard metadata explicitly"
    )]
    pub fn lookup_class_constant_static_property(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        kind: ClassConstantStaticPropertyCacheKind,
        resolved_class: &str,
        member: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> (
        Option<ClassConstantStaticPropertyCacheTarget>,
        InlineCacheObservation,
    ) {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::ClassConstantStaticProperty,
        );
        let Some(slot) = self.slots.get_mut(&key) else {
            return (
                None,
                with_kind(
                    InlineCacheKind::ClassConstantStaticProperty,
                    InlineCacheObservation::miss(),
                ),
            );
        };
        if slot.state == InlineCacheState::Disabled {
            return (
                None,
                with_kind(
                    InlineCacheKind::ClassConstantStaticProperty,
                    disabled_slot_observation(),
                ),
            );
        }
        let Some(cached_kind) = slot.class_static_kind else {
            let observation = record_slot_miss(slot);
            return (
                None,
                with_kind(InlineCacheKind::ClassConstantStaticProperty, observation),
            );
        };
        let cached_class = slot.class_static_resolved_class.as_deref();
        let cached_member = slot.class_static_member.as_deref();
        let cached_scope = slot.class_static_scope.as_deref();
        if cached_kind != kind
            || cached_class != Some(resolved_class)
            || cached_member != Some(member)
            || cached_scope.is_some_and(|cached| Some(cached) != scope)
        {
            let observation = record_slot_guard_failure(slot);
            return (
                None,
                with_kind(InlineCacheKind::ClassConstantStaticProperty, observation),
            );
        }
        if slot.epoch != epoch {
            let observation = record_slot_invalidation(slot);
            return (
                None,
                with_kind(InlineCacheKind::ClassConstantStaticProperty, observation),
            );
        }
        let Some(target) = slot.class_static_target.clone() else {
            let observation = record_slot_miss(slot);
            return (
                None,
                with_kind(InlineCacheKind::ClassConstantStaticProperty, observation),
            );
        };
        let observation = record_slot_hit(slot);
        (
            Some(target),
            with_kind(InlineCacheKind::ClassConstantStaticProperty, observation),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "inline cache install APIs take the complete cache key and target metadata explicitly"
    )]
    pub fn install_class_constant_static_property(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        kind: ClassConstantStaticPropertyCacheKind,
        resolved_class: &str,
        member: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
        target: ClassConstantStaticPropertyCacheTarget,
    ) {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::ClassConstantStaticProperty,
        );
        if let Some(slot) = self.slots.get_mut(&key) {
            if slot.state == InlineCacheState::Disabled {
                return;
            }
            slot.state = InlineCacheState::Monomorphic;
            slot.epoch = epoch;
            slot.class_static_kind = Some(kind);
            slot.class_static_resolved_class = Some(resolved_class.to_owned());
            slot.class_static_member = Some(member.to_owned());
            slot.class_static_scope = scope.map(str::to_owned);
            slot.class_static_target = Some(target);
        }
    }

    pub fn lookup_include_path(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        request: &IncludePathCacheKey,
        epoch: InvalidationEpoch,
    ) -> (Option<IncludePathCacheTarget>, InlineCacheObservation) {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::IncludePath,
        );
        let include_path_observation =
            |observation: InlineCacheObservation| InlineCacheObservation {
                kind: Some(InlineCacheKind::IncludePath),
                ..observation
            };
        let Some(slot) = self.slots.get_mut(&key) else {
            return (
                None,
                include_path_observation(InlineCacheObservation::miss()),
            );
        };
        let Some(cached_request) = slot.include_path_key.as_ref() else {
            slot.stats.misses = slot.stats.misses.saturating_add(1);
            return (
                None,
                include_path_observation(InlineCacheObservation::miss()),
            );
        };
        if cached_request != request {
            slot.stats.guard_failures = slot.stats.guard_failures.saturating_add(1);
            slot.stats.misses = slot.stats.misses.saturating_add(1);
            return (
                None,
                include_path_observation(InlineCacheObservation::guard_failure()),
            );
        }
        if slot.epoch != epoch {
            slot.stats.invalidations = slot.stats.invalidations.saturating_add(1);
            slot.stats.misses = slot.stats.misses.saturating_add(1);
            slot.state = InlineCacheState::Cold;
            slot.include_path_target = None;
            return (
                None,
                include_path_observation(InlineCacheObservation::invalidation()),
            );
        }
        let Some(target) = slot.include_path_target.clone() else {
            slot.stats.misses = slot.stats.misses.saturating_add(1);
            return (
                None,
                include_path_observation(InlineCacheObservation::miss()),
            );
        };
        (
            Some(target),
            include_path_observation(InlineCacheObservation::empty()),
        )
    }

    pub fn record_include_path_hit(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) -> InlineCacheObservation {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::IncludePath,
        );
        if let Some(slot) = self.slots.get_mut(&key) {
            slot.stats.hits = slot.stats.hits.saturating_add(1);
        }
        InlineCacheObservation {
            kind: Some(InlineCacheKind::IncludePath),
            ..InlineCacheObservation::hit()
        }
    }

    pub fn invalidate_include_path(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) -> InlineCacheObservation {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::IncludePath,
        );
        if let Some(slot) = self.slots.get_mut(&key) {
            slot.stats.invalidations = slot.stats.invalidations.saturating_add(1);
            slot.stats.misses = slot.stats.misses.saturating_add(1);
            slot.state = InlineCacheState::Cold;
            slot.include_path_target = None;
        }
        InlineCacheObservation {
            kind: Some(InlineCacheKind::IncludePath),
            ..InlineCacheObservation::invalidation()
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "inline cache install APIs take the complete cache key and target metadata explicitly"
    )]
    pub fn install_include_path(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        request: IncludePathCacheKey,
        epoch: InvalidationEpoch,
        target: IncludePathCacheTarget,
    ) {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::IncludePath,
        );
        if let Some(slot) = self.slots.get_mut(&key) {
            slot.state = InlineCacheState::Monomorphic;
            slot.epoch = epoch;
            slot.include_path_key = Some(request);
            slot.include_path_target = Some(target);
        }
    }

    pub fn lookup_autoload_class_lookup(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        request: &AutoloadClassLookupCacheKey,
        epochs: AutoloadClassLookupEpochs,
    ) -> (
        Option<AutoloadClassLookupCacheTarget>,
        InlineCacheObservation,
    ) {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::AutoloadClassLookup,
        );
        let autoload_observation = |observation: InlineCacheObservation| InlineCacheObservation {
            kind: Some(InlineCacheKind::AutoloadClassLookup),
            ..observation
        };
        let Some(slot) = self.slots.get_mut(&key) else {
            return (None, autoload_observation(InlineCacheObservation::miss()));
        };
        let Some(cached_request) = slot.autoload_class_lookup_key.as_ref() else {
            slot.stats.misses = slot.stats.misses.saturating_add(1);
            return (None, autoload_observation(InlineCacheObservation::miss()));
        };
        if cached_request != request {
            slot.stats.guard_failures = slot.stats.guard_failures.saturating_add(1);
            slot.stats.misses = slot.stats.misses.saturating_add(1);
            return (
                None,
                autoload_observation(InlineCacheObservation::guard_failure()),
            );
        }
        if slot.autoload_class_lookup_epochs != Some(epochs) {
            slot.stats.invalidations = slot.stats.invalidations.saturating_add(1);
            slot.stats.misses = slot.stats.misses.saturating_add(1);
            slot.state = InlineCacheState::Cold;
            slot.autoload_class_lookup_target = None;
            return (
                None,
                autoload_observation(InlineCacheObservation::invalidation()),
            );
        }
        let Some(target) = slot.autoload_class_lookup_target.clone() else {
            slot.stats.misses = slot.stats.misses.saturating_add(1);
            return (None, autoload_observation(InlineCacheObservation::miss()));
        };
        slot.stats.hits = slot.stats.hits.saturating_add(1);
        (
            Some(target),
            autoload_observation(InlineCacheObservation::hit()),
        )
    }

    pub fn invalidate_autoload_class_lookup(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) -> InlineCacheObservation {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::AutoloadClassLookup,
        );
        if let Some(slot) = self.slots.get_mut(&key) {
            slot.stats.invalidations = slot.stats.invalidations.saturating_add(1);
            slot.stats.misses = slot.stats.misses.saturating_add(1);
            slot.state = InlineCacheState::Cold;
            slot.autoload_class_lookup_target = None;
        }
        InlineCacheObservation {
            kind: Some(InlineCacheKind::AutoloadClassLookup),
            ..InlineCacheObservation::invalidation()
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "inline cache install APIs take the complete cache key and target metadata explicitly"
    )]
    pub fn install_autoload_class_lookup(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        request: AutoloadClassLookupCacheKey,
        epochs: AutoloadClassLookupEpochs,
        target: AutoloadClassLookupCacheTarget,
    ) {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::AutoloadClassLookup,
        );
        if let Some(slot) = self.slots.get_mut(&key) {
            slot.state = InlineCacheState::Monomorphic;
            slot.autoload_class_lookup_key = Some(request);
            slot.autoload_class_lookup_epochs = Some(epochs);
            slot.autoload_class_lookup_target = Some(target);
        }
    }
}

#[must_use]
pub fn inline_cache_kind_for_instruction(kind: &InstructionKind) -> Option<InlineCacheKind> {
    match kind {
        InstructionKind::BindReferenceFromCall { .. }
        | InstructionKind::CallFunction { .. }
        | InstructionKind::CallClosure { .. }
        | InstructionKind::CallCallable { .. }
        | InstructionKind::Pipe { .. } => Some(InlineCacheKind::FunctionCall),
        InstructionKind::CallMethod { .. } | InstructionKind::CallStaticMethod { .. } => {
            Some(InlineCacheKind::MethodCall)
        }
        InstructionKind::FetchProperty { .. } => Some(InlineCacheKind::PropertyFetch),
        InstructionKind::AssignProperty { .. } => Some(InlineCacheKind::PropertyAssign),
        InstructionKind::FetchStaticProperty { .. }
        | InstructionKind::FetchClassConstant { .. } => {
            Some(InlineCacheKind::ClassConstantStaticProperty)
        }
        InstructionKind::Include { .. } => Some(InlineCacheKind::IncludePath),
        InstructionKind::InstanceOf { .. }
        | InstructionKind::DynamicInstanceOf { .. }
        | InstructionKind::NewObject { .. } => Some(InlineCacheKind::AutoloadClassLookup),
        InstructionKind::FetchDim { .. } | InstructionKind::ArrayGet { .. } => {
            Some(InlineCacheKind::DimFetch)
        }
        _ => None,
    }
}

fn inline_cache_key(
    unit_key: u64,
    function: FunctionId,
    block: BlockId,
    instruction: InstrId,
    kind: InlineCacheKind,
) -> InlineCacheKey {
    InlineCacheKey {
        unit_key,
        function: function.raw(),
        block: block.raw(),
        instruction: instruction.raw(),
        kind,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AutoloadClassLookupCacheKey, AutoloadClassLookupCacheTarget, AutoloadClassLookupEpochs,
        AutoloadClassLookupKind, ClassConstantStaticPropertyCacheKind,
        ClassConstantStaticPropertyCacheTarget, FunctionCallBuiltinKind,
        FunctionCallBuiltinMetadata, FunctionCallCacheTarget, FunctionCallShape,
        IncludePathCacheKey, IncludePathCacheTarget, InlineCacheKind, InlineCacheState,
        InlineCacheTable, InvalidationEpoch, MethodCallCacheTarget, MethodCallGuardMetadata,
        MethodCallResolvedTarget, MethodCallShape, PropertyFetchCacheTarget,
        PropertyFetchLayoutMetadata, PropertyFetchResolvedTarget,
    };
    use crate::include::IncludePathFileFingerprint;
    use php_ir::ids::{BlockId, FunctionId, InstrId};
    use std::path::PathBuf;

    fn positional_shape(arity: u32) -> FunctionCallShape {
        FunctionCallShape {
            arity,
            named_arguments: Vec::new(),
            by_ref_arguments: vec![false; arity as usize],
        }
    }

    fn method_target(
        receiver_class: &str,
        receiver_class_id: u32,
        declaring_class: &str,
        function: FunctionId,
        epoch: InvalidationEpoch,
    ) -> MethodCallCacheTarget {
        MethodCallCacheTarget::CurrentUnit {
            target: Box::new(MethodCallResolvedTarget {
                receiver_class: receiver_class.to_owned(),
                declaring_class: declaring_class.to_owned(),
                function,
                guard: MethodCallGuardMetadata {
                    receiver_class_id,
                    class_layout_epoch: epoch.raw(),
                    method_table_epoch: epoch.raw(),
                    method_slot_index: Some(0),
                    visibility_context: None,
                    method_is_final: false,
                    method_is_private: false,
                    method_is_static: false,
                    receiver_has_override: false,
                    argument_shape: MethodCallShape {
                        arity: 0,
                        named_arguments: Vec::new(),
                        by_ref_arguments: Vec::new(),
                    },
                    by_ref_compatible: true,
                    has_magic_call: false,
                },
            }),
        }
    }

    fn builtin_metadata(name: &str) -> FunctionCallBuiltinMetadata {
        FunctionCallBuiltinMetadata {
            implementation_id: format!("internal_registry:{name}"),
            version: 1,
        }
    }

    fn property_layout(class_id: u32) -> PropertyFetchLayoutMetadata {
        PropertyFetchLayoutMetadata {
            class_id,
            layout_version: 6,
            property_slot_index: Some(0),
            visibility_context: None,
            typed_property_initialized: true,
            has_property_hooks: false,
            has_magic_get: false,
            dynamic_property_fallback: false,
        }
    }

    fn property_target(
        receiver_class: &str,
        declaring_class: &str,
        class_id: u32,
    ) -> Box<PropertyFetchResolvedTarget> {
        Box::new(PropertyFetchResolvedTarget {
            receiver_class: receiver_class.to_owned(),
            declaring_class: declaring_class.to_owned(),
            property: "value".to_owned(),
            storage_name: "value".to_owned(),
            layout: property_layout(class_id),
        })
    }

    #[test]
    fn inline_cache_table_allocates_one_stable_slot_per_instruction_kind() {
        let function = FunctionId::new(0);
        let block = BlockId::new(1);
        let instruction = InstrId::new(2);
        let mut table = InlineCacheTable::default();

        let first = table.observe_slot(
            17,
            function,
            block,
            instruction,
            InlineCacheKind::FunctionCall,
        );
        let second = table.observe_slot(
            17,
            function,
            block,
            instruction,
            InlineCacheKind::FunctionCall,
        );
        let third = table.observe_slot(17, function, block, instruction, InlineCacheKind::DimFetch);

        assert!(first.candidate);
        assert!(first.slot_allocated);
        assert!(second.candidate);
        assert!(!second.slot_allocated);
        assert!(third.slot_allocated);
        assert_eq!(table.slot_count(), 2);
    }

    #[test]
    fn inline_cache_slot_state_starts_cold() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(
            1,
            function,
            block,
            instruction,
            InlineCacheKind::PropertyFetch,
        );
        let slot = table.slots.values().next().expect("slot");

        assert_eq!(slot.id.raw(), 0);
        assert_eq!(slot.state, InlineCacheState::Cold);
        assert_eq!(slot.epoch.raw(), 0);
        assert_eq!(slot.stats.hits, 0);
    }

    #[test]
    fn function_call_cache_hits_same_name_and_epoch() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(
            7,
            function,
            block,
            instruction,
            InlineCacheKind::FunctionCall,
        );
        table.install_function_call(
            7,
            function,
            block,
            instruction,
            "strlen",
            InvalidationEpoch::new(3),
            positional_shape(1),
            Some(builtin_metadata("strlen")),
            FunctionCallCacheTarget::Builtin {
                kind: FunctionCallBuiltinKind::InternalRegistry,
                name: "strlen".to_owned(),
            },
        );
        let (target, event) = table.lookup_function_call(
            7,
            function,
            block,
            instruction,
            "strlen",
            InvalidationEpoch::new(3),
            &positional_shape(1),
            Some(&builtin_metadata("strlen")),
        );

        assert!(target.is_some());
        assert!(event.hit);
        assert!(!event.miss);
    }

    #[test]
    fn function_call_cache_invalidates_on_epoch_change() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(
            7,
            function,
            block,
            instruction,
            InlineCacheKind::FunctionCall,
        );
        table.install_function_call(
            7,
            function,
            block,
            instruction,
            "perf_fn",
            InvalidationEpoch::new(1),
            positional_shape(0),
            None,
            FunctionCallCacheTarget::CurrentUnit { function },
        );
        let (target, event) = table.lookup_function_call(
            7,
            function,
            block,
            instruction,
            "perf_fn",
            InvalidationEpoch::new(2),
            &positional_shape(0),
            None,
        );

        assert!(target.is_none());
        assert!(event.invalidation);
        assert!(event.miss);
    }

    #[test]
    fn function_call_cache_guards_call_shape_and_builtin_metadata() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();
        let shape = FunctionCallShape {
            arity: 2,
            named_arguments: vec!["left".to_owned(), "right".to_owned()],
            by_ref_arguments: vec![false, false],
        };

        table.observe_slot(
            7,
            function,
            block,
            instruction,
            InlineCacheKind::FunctionCall,
        );
        table.install_function_call(
            7,
            function,
            block,
            instruction,
            "strlen",
            InvalidationEpoch::new(1),
            shape.clone(),
            Some(builtin_metadata("strlen")),
            FunctionCallCacheTarget::Builtin {
                kind: FunctionCallBuiltinKind::InternalRegistry,
                name: "strlen".to_owned(),
            },
        );

        let wrong_shape = positional_shape(2);
        let (target, event) = table.lookup_function_call(
            7,
            function,
            block,
            instruction,
            "strlen",
            InvalidationEpoch::new(1),
            &wrong_shape,
            Some(&builtin_metadata("strlen")),
        );
        assert!(target.is_none());
        assert!(event.guard_failure);

        table.install_function_call(
            7,
            function,
            block,
            instruction,
            "strlen",
            InvalidationEpoch::new(1),
            shape.clone(),
            Some(builtin_metadata("strlen")),
            FunctionCallCacheTarget::Builtin {
                kind: FunctionCallBuiltinKind::InternalRegistry,
                name: "strlen".to_owned(),
            },
        );
        let wrong_metadata = FunctionCallBuiltinMetadata {
            implementation_id: "InternalRegistry:strlen".to_owned(),
            version: 2,
        };
        let (target, event) = table.lookup_function_call(
            7,
            function,
            block,
            instruction,
            "strlen",
            InvalidationEpoch::new(1),
            &shape,
            Some(&wrong_metadata),
        );
        assert!(target.is_none());
        assert!(event.guard_failure);
    }

    #[test]
    fn function_call_cache_type_changes_reach_capped_megamorphic_fallback() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(
            7,
            function,
            block,
            instruction,
            InlineCacheKind::FunctionCall,
        );
        table.install_function_call(
            7,
            function,
            block,
            instruction,
            "perf_fn_a",
            InvalidationEpoch::new(1),
            positional_shape(0),
            None,
            FunctionCallCacheTarget::CurrentUnit { function },
        );

        let mut saw_megamorphic = false;
        for name in ["perf_fn_b", "perf_fn_c", "perf_fn_d", "perf_fn_e"] {
            let (target, event) = table.lookup_function_call(
                7,
                function,
                block,
                instruction,
                name,
                InvalidationEpoch::new(1),
                &positional_shape(0),
                None,
            );
            assert!(target.is_none());
            assert!(event.fallback_call);
            saw_megamorphic |= event.megamorphic;
            assert!(!event.disabled);
            table.install_function_call(
                7,
                function,
                block,
                instruction,
                name,
                InvalidationEpoch::new(1),
                positional_shape(0),
                None,
                FunctionCallCacheTarget::CurrentUnit { function },
            );
        }

        let slot = table.slots.values().next().expect("slot");
        assert!(saw_megamorphic);
        assert_eq!(slot.state, InlineCacheState::Megamorphic);
        assert_eq!(slot.stats.guard_failures, 0);
        assert_eq!(slot.stats.protocol.fallback_calls, 4);
        assert_eq!(slot.stats.megamorphic_transitions, 1);
        assert_eq!(slot.stats.disabled_transitions, 0);
        assert!(slot.function_call_target.is_none());
    }

    #[test]
    fn function_call_cache_hits_polymorphic_entries_before_cap() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(
            7,
            function,
            block,
            instruction,
            InlineCacheKind::FunctionCall,
        );
        for (index, name) in ["perf_fn_a", "perf_fn_b"].iter().enumerate() {
            table.install_function_call(
                7,
                function,
                block,
                instruction,
                name,
                InvalidationEpoch::new(1),
                positional_shape(0),
                None,
                FunctionCallCacheTarget::CurrentUnit {
                    function: FunctionId::new(index as u32),
                },
            );
        }

        let (target, event) = table.lookup_function_call(
            7,
            function,
            block,
            instruction,
            "perf_fn_b",
            InvalidationEpoch::new(1),
            &positional_shape(0),
            None,
        );

        assert_eq!(
            target,
            Some(FunctionCallCacheTarget::CurrentUnit {
                function: FunctionId::new(1)
            })
        );
        assert!(event.hit);
        assert!(event.polymorphic);
        assert!(!event.guard_failure);
    }

    #[test]
    fn method_call_cache_hits_same_receiver_scope_and_epoch() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(9, function, block, instruction, InlineCacheKind::MethodCall);
        table.install_method_call(
            9,
            function,
            block,
            instruction,
            "value",
            "performancemethod",
            Some("performancecaller"),
            InvalidationEpoch::new(4),
            method_target(
                "performancemethod",
                7,
                "PerfMethod",
                function,
                InvalidationEpoch::new(4),
            ),
        );
        let (target, event) = table.lookup_method_call(
            9,
            function,
            block,
            instruction,
            "value",
            "performancemethod",
            Some("performancecaller"),
            InvalidationEpoch::new(4),
        );

        assert!(target.is_some());
        assert_eq!(event.kind, Some(InlineCacheKind::MethodCall));
        assert!(event.hit);
        assert!(!event.miss);
    }

    #[test]
    fn method_call_cache_guard_fails_on_receiver_change() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(9, function, block, instruction, InlineCacheKind::MethodCall);
        table.install_method_call(
            9,
            function,
            block,
            instruction,
            "value",
            "performancemethoda",
            None,
            InvalidationEpoch::new(4),
            method_target(
                "performancemethoda",
                7,
                "PerfMethodA",
                function,
                InvalidationEpoch::new(4),
            ),
        );
        let (target, event) = table.lookup_method_call(
            9,
            function,
            block,
            instruction,
            "value",
            "performancemethodb",
            None,
            InvalidationEpoch::new(4),
        );

        assert!(target.is_none());
        assert_eq!(event.kind, Some(InlineCacheKind::MethodCall));
        assert!(event.guard_failure);
        assert!(event.miss);
    }

    #[test]
    fn method_call_cache_invalidates_on_epoch_change() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(9, function, block, instruction, InlineCacheKind::MethodCall);
        table.install_method_call(
            9,
            function,
            block,
            instruction,
            "value",
            "performancemethod",
            None,
            InvalidationEpoch::new(4),
            method_target(
                "performancemethod",
                7,
                "PerfMethod",
                function,
                InvalidationEpoch::new(4),
            ),
        );
        let (target, event) = table.lookup_method_call(
            9,
            function,
            block,
            instruction,
            "value",
            "performancemethod",
            None,
            InvalidationEpoch::new(5),
        );

        assert!(target.is_none());
        assert_eq!(event.kind, Some(InlineCacheKind::MethodCall));
        assert!(event.invalidation);
        assert!(event.miss);
    }

    #[test]
    fn method_call_cache_records_polymorphic_receiver_targets() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(9, function, block, instruction, InlineCacheKind::MethodCall);
        for (receiver, function_id) in [
            ("performancemethoda", FunctionId::new(1)),
            ("performancemethodb", FunctionId::new(2)),
        ] {
            table.install_method_call(
                9,
                function,
                block,
                instruction,
                "value",
                receiver,
                None,
                InvalidationEpoch::new(4),
                method_target(
                    receiver,
                    function_id.raw(),
                    receiver,
                    function_id,
                    InvalidationEpoch::new(4),
                ),
            );
        }

        let (target, event) = table.lookup_method_call(
            9,
            function,
            block,
            instruction,
            "value",
            "performancemethodb",
            None,
            InvalidationEpoch::new(4),
        );

        assert!(target.is_some());
        assert!(event.hit);
        assert!(event.polymorphic);
        assert!(!event.monomorphic);
        let slot = table.slots.values().next().expect("slot");
        assert_eq!(slot.state, InlineCacheState::Polymorphic);
        assert_eq!(slot.method_call_polymorphic_entries.len(), 2);
    }

    #[test]
    fn method_call_cache_overflow_reaches_megamorphic_fallback() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(9, function, block, instruction, InlineCacheKind::MethodCall);
        for receiver in [
            "performancemethoda",
            "performancemethodb",
            "performancemethodc",
            "performancemethodd",
            "performancemethode",
        ] {
            table.install_method_call(
                9,
                function,
                block,
                instruction,
                "value",
                receiver,
                None,
                InvalidationEpoch::new(4),
                method_target(receiver, 7, receiver, function, InvalidationEpoch::new(4)),
            );
        }

        let (target, event) = table.lookup_method_call(
            9,
            function,
            block,
            instruction,
            "value",
            "performancemethoda",
            None,
            InvalidationEpoch::new(4),
        );

        assert!(target.is_none());
        assert!(event.megamorphic);
        assert!(event.fallback_call);
        let slot = table.slots.values().next().expect("slot");
        assert_eq!(slot.state, InlineCacheState::Megamorphic);
        assert!(slot.method_call_polymorphic_entries.is_empty());
    }

    #[test]
    fn property_fetch_cache_hits_same_receiver_scope_and_epoch() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(
            11,
            function,
            block,
            instruction,
            InlineCacheKind::PropertyFetch,
        );
        table.install_property_fetch(
            11,
            function,
            block,
            instruction,
            "value",
            "performancebox",
            None,
            InvalidationEpoch::new(6),
            PropertyFetchCacheTarget::CurrentUnit {
                target: property_target("performancebox", "PerfBox", 11),
            },
        );
        let (target, event) = table.lookup_property_fetch(
            11,
            function,
            block,
            instruction,
            "value",
            "performancebox",
            Some("different_scope_allowed_for_public"),
            InvalidationEpoch::new(6),
        );

        assert!(target.is_some());
        assert_eq!(event.kind, Some(InlineCacheKind::PropertyFetch));
        assert!(event.hit);
        assert!(!event.miss);
    }

    #[test]
    fn property_fetch_cache_guard_fails_on_receiver_change() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(
            11,
            function,
            block,
            instruction,
            InlineCacheKind::PropertyFetch,
        );
        table.install_property_fetch(
            11,
            function,
            block,
            instruction,
            "value",
            "performanceboxa",
            None,
            InvalidationEpoch::new(6),
            PropertyFetchCacheTarget::CurrentUnit {
                target: property_target("performanceboxa", "PerfBoxA", 12),
            },
        );
        let (target, event) = table.lookup_property_fetch(
            11,
            function,
            block,
            instruction,
            "value",
            "performanceboxb",
            None,
            InvalidationEpoch::new(6),
        );

        assert!(target.is_none());
        assert_eq!(event.kind, Some(InlineCacheKind::PropertyFetch));
        assert!(event.guard_failure);
        assert!(event.miss);
    }

    #[test]
    fn property_fetch_cache_invalidates_on_epoch_change() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(
            11,
            function,
            block,
            instruction,
            InlineCacheKind::PropertyFetch,
        );
        table.install_property_fetch(
            11,
            function,
            block,
            instruction,
            "value",
            "performancebox",
            None,
            InvalidationEpoch::new(6),
            PropertyFetchCacheTarget::CurrentUnit {
                target: property_target("performancebox", "PerfBox", 13),
            },
        );
        let (target, event) = table.lookup_property_fetch(
            11,
            function,
            block,
            instruction,
            "value",
            "performancebox",
            None,
            InvalidationEpoch::new(7),
        );

        assert!(target.is_none());
        assert_eq!(event.kind, Some(InlineCacheKind::PropertyFetch));
        assert!(event.invalidation);
        assert!(event.miss);
    }

    #[test]
    fn property_fetch_cache_records_polymorphic_receiver_targets() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(
            11,
            function,
            block,
            instruction,
            InlineCacheKind::PropertyFetch,
        );
        for receiver in ["performanceboxa", "performanceboxb"] {
            table.install_property_fetch(
                11,
                function,
                block,
                instruction,
                "value",
                receiver,
                None,
                InvalidationEpoch::new(6),
                PropertyFetchCacheTarget::CurrentUnit {
                    target: property_target(receiver, receiver, 14),
                },
            );
        }

        let (target, event) = table.lookup_property_fetch(
            11,
            function,
            block,
            instruction,
            "value",
            "performanceboxb",
            Some("public_scope"),
            InvalidationEpoch::new(6),
        );

        assert!(target.is_some());
        assert!(event.hit);
        assert!(event.polymorphic);
        assert!(!event.monomorphic);
        let slot = table.slots.values().next().expect("slot");
        assert_eq!(slot.state, InlineCacheState::Polymorphic);
        assert_eq!(slot.property_fetch_polymorphic_entries.len(), 2);
    }

    #[test]
    fn property_fetch_cache_overflow_reaches_megamorphic_fallback() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(
            11,
            function,
            block,
            instruction,
            InlineCacheKind::PropertyFetch,
        );
        for receiver in [
            "performanceboxa",
            "performanceboxb",
            "performanceboxc",
            "performanceboxd",
            "performanceboxe",
        ] {
            table.install_property_fetch(
                11,
                function,
                block,
                instruction,
                "value",
                receiver,
                None,
                InvalidationEpoch::new(6),
                PropertyFetchCacheTarget::CurrentUnit {
                    target: property_target(receiver, receiver, 15),
                },
            );
        }

        let (target, event) = table.lookup_property_fetch(
            11,
            function,
            block,
            instruction,
            "value",
            "performanceboxa",
            None,
            InvalidationEpoch::new(6),
        );

        assert!(target.is_none());
        assert!(event.megamorphic);
        assert!(event.fallback_call);
        let slot = table.slots.values().next().expect("slot");
        assert_eq!(slot.state, InlineCacheState::Megamorphic);
        assert!(slot.property_fetch_polymorphic_entries.is_empty());
    }

    #[test]
    fn class_static_cache_hits_same_class_member_scope_and_epoch() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(
            13,
            function,
            block,
            instruction,
            InlineCacheKind::ClassConstantStaticProperty,
        );
        table.install_class_constant_static_property(
            13,
            function,
            block,
            instruction,
            ClassConstantStaticPropertyCacheKind::ClassConstant,
            "performanceclass",
            "VALUE",
            None,
            InvalidationEpoch::new(8),
            ClassConstantStaticPropertyCacheTarget::CurrentUnit {
                kind: ClassConstantStaticPropertyCacheKind::ClassConstant,
                resolved_class: "performanceclass".to_owned(),
                declaring_class: "PerfClass".to_owned(),
                member: "VALUE".to_owned(),
            },
        );
        let (target, event) = table.lookup_class_constant_static_property(
            13,
            function,
            block,
            instruction,
            ClassConstantStaticPropertyCacheKind::ClassConstant,
            "performanceclass",
            "VALUE",
            Some("public_scope_ignored"),
            InvalidationEpoch::new(8),
        );

        assert!(target.is_some());
        assert_eq!(
            event.kind,
            Some(InlineCacheKind::ClassConstantStaticProperty)
        );
        assert!(event.hit);
        assert!(!event.miss);
    }

    #[test]
    fn class_static_cache_guard_fails_on_resolved_class_change() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(
            13,
            function,
            block,
            instruction,
            InlineCacheKind::ClassConstantStaticProperty,
        );
        table.install_class_constant_static_property(
            13,
            function,
            block,
            instruction,
            ClassConstantStaticPropertyCacheKind::StaticProperty,
            "performancea",
            "value",
            None,
            InvalidationEpoch::new(8),
            ClassConstantStaticPropertyCacheTarget::CurrentUnit {
                kind: ClassConstantStaticPropertyCacheKind::StaticProperty,
                resolved_class: "performancea".to_owned(),
                declaring_class: "PerfA".to_owned(),
                member: "value".to_owned(),
            },
        );
        let (target, event) = table.lookup_class_constant_static_property(
            13,
            function,
            block,
            instruction,
            ClassConstantStaticPropertyCacheKind::StaticProperty,
            "performanceb",
            "value",
            None,
            InvalidationEpoch::new(8),
        );

        assert!(target.is_none());
        assert_eq!(
            event.kind,
            Some(InlineCacheKind::ClassConstantStaticProperty)
        );
        assert!(event.guard_failure);
        assert!(event.miss);
    }

    #[test]
    fn class_static_cache_invalidates_on_epoch_change() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();

        table.observe_slot(
            13,
            function,
            block,
            instruction,
            InlineCacheKind::ClassConstantStaticProperty,
        );
        table.install_class_constant_static_property(
            13,
            function,
            block,
            instruction,
            ClassConstantStaticPropertyCacheKind::EnumCase,
            "performanceenum",
            "Ready",
            None,
            InvalidationEpoch::new(8),
            ClassConstantStaticPropertyCacheTarget::CurrentUnit {
                kind: ClassConstantStaticPropertyCacheKind::EnumCase,
                resolved_class: "performanceenum".to_owned(),
                declaring_class: "PerfEnum".to_owned(),
                member: "Ready".to_owned(),
            },
        );
        let (target, event) = table.lookup_class_constant_static_property(
            13,
            function,
            block,
            instruction,
            ClassConstantStaticPropertyCacheKind::EnumCase,
            "performanceenum",
            "Ready",
            None,
            InvalidationEpoch::new(9),
        );

        assert!(target.is_none());
        assert_eq!(
            event.kind,
            Some(InlineCacheKind::ClassConstantStaticProperty)
        );
        assert!(event.invalidation);
        assert!(event.miss);
    }

    #[test]
    fn autoload_class_lookup_cache_hits_same_guard_and_epochs() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();
        let request = AutoloadClassLookupCacheKey {
            kind: AutoloadClassLookupKind::Class,
            normalized_name: "performance\\cache\\thing".to_owned(),
            autoload_enabled: true,
            autoload_stack_depth: 0,
            include_path_config: "vendor".to_owned(),
            composer_map_fingerprint: Some("classmap:1".to_owned()),
        };
        let epochs = AutoloadClassLookupEpochs {
            autoload_stack_epoch: 1,
            class_table_epoch: 2,
            include_config_epoch: 3,
        };

        table.observe_slot(
            17,
            function,
            block,
            instruction,
            InlineCacheKind::AutoloadClassLookup,
        );
        table.install_autoload_class_lookup(
            17,
            function,
            block,
            instruction,
            request.clone(),
            epochs,
            AutoloadClassLookupCacheTarget::Positive {
                display_name: "Perf\\Cache\\Thing".to_owned(),
            },
        );
        let (target, event) =
            table.lookup_autoload_class_lookup(17, function, block, instruction, &request, epochs);

        assert_eq!(
            target,
            Some(AutoloadClassLookupCacheTarget::Positive {
                display_name: "Perf\\Cache\\Thing".to_owned(),
            })
        );
        assert_eq!(event.kind, Some(InlineCacheKind::AutoloadClassLookup));
        assert!(event.hit);
        assert!(!event.miss);
    }

    #[test]
    fn autoload_class_lookup_cache_guard_fails_on_lookup_kind_change() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();
        let request = AutoloadClassLookupCacheKey {
            kind: AutoloadClassLookupKind::Class,
            normalized_name: "performance\\cache\\thing".to_owned(),
            autoload_enabled: false,
            autoload_stack_depth: 0,
            include_path_config: ".".to_owned(),
            composer_map_fingerprint: None,
        };
        let changed = AutoloadClassLookupCacheKey {
            kind: AutoloadClassLookupKind::Interface,
            ..request.clone()
        };
        let epochs = AutoloadClassLookupEpochs::default();

        table.observe_slot(
            17,
            function,
            block,
            instruction,
            InlineCacheKind::AutoloadClassLookup,
        );
        table.install_autoload_class_lookup(
            17,
            function,
            block,
            instruction,
            request,
            epochs,
            AutoloadClassLookupCacheTarget::Negative,
        );
        let (target, event) =
            table.lookup_autoload_class_lookup(17, function, block, instruction, &changed, epochs);

        assert!(target.is_none());
        assert_eq!(event.kind, Some(InlineCacheKind::AutoloadClassLookup));
        assert!(event.guard_failure);
        assert!(event.miss);
    }

    #[test]
    fn autoload_class_lookup_cache_invalidates_on_class_table_epoch_change() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();
        let request = AutoloadClassLookupCacheKey {
            kind: AutoloadClassLookupKind::Class,
            normalized_name: "performance\\cache\\late".to_owned(),
            autoload_enabled: false,
            autoload_stack_depth: 0,
            include_path_config: ".".to_owned(),
            composer_map_fingerprint: None,
        };

        table.observe_slot(
            17,
            function,
            block,
            instruction,
            InlineCacheKind::AutoloadClassLookup,
        );
        table.install_autoload_class_lookup(
            17,
            function,
            block,
            instruction,
            request.clone(),
            AutoloadClassLookupEpochs {
                autoload_stack_epoch: 0,
                class_table_epoch: 1,
                include_config_epoch: 0,
            },
            AutoloadClassLookupCacheTarget::Negative,
        );
        let (target, event) = table.lookup_autoload_class_lookup(
            17,
            function,
            block,
            instruction,
            &request,
            AutoloadClassLookupEpochs {
                autoload_stack_epoch: 0,
                class_table_epoch: 2,
                include_config_epoch: 0,
            },
        );

        assert!(target.is_none());
        assert_eq!(event.kind, Some(InlineCacheKind::AutoloadClassLookup));
        assert!(event.invalidation);
        assert!(event.miss);
    }

    #[test]
    fn include_path_cache_hits_same_request_and_epoch_after_validation() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();
        let request = IncludePathCacheKey {
            path: "lib.php".to_owned(),
            include_path: vec![PathBuf::from("src")],
            cwd: PathBuf::from("/repo"),
            calling_file_directory: Some(PathBuf::from("/repo/app")),
        };
        let target = IncludePathCacheTarget {
            canonical_path: PathBuf::from("/repo/src/lib.php"),
            fingerprint: IncludePathFileFingerprint {
                len: 17,
                modified_unix_nanos: Some(10),
                readonly: false,
            },
        };

        table.observe_slot(
            15,
            function,
            block,
            instruction,
            InlineCacheKind::IncludePath,
        );
        table.install_include_path(
            15,
            function,
            block,
            instruction,
            request.clone(),
            InvalidationEpoch::new(2),
            target.clone(),
        );
        let (cached, probe) = table.lookup_include_path(
            15,
            function,
            block,
            instruction,
            &request,
            InvalidationEpoch::new(2),
        );
        let hit = table.record_include_path_hit(15, function, block, instruction);

        assert_eq!(cached, Some(target));
        assert_eq!(probe.kind, Some(InlineCacheKind::IncludePath));
        assert!(!probe.hit);
        assert!(hit.hit);
        assert!(!hit.miss);
    }

    #[test]
    fn include_path_cache_guard_fails_on_include_path_order_change() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();
        let request = IncludePathCacheKey {
            path: "lib.php".to_owned(),
            include_path: vec![PathBuf::from("first"), PathBuf::from("second")],
            cwd: PathBuf::from("/repo"),
            calling_file_directory: Some(PathBuf::from("/repo/app")),
        };
        let changed = IncludePathCacheKey {
            include_path: vec![PathBuf::from("second"), PathBuf::from("first")],
            ..request.clone()
        };

        table.observe_slot(
            15,
            function,
            block,
            instruction,
            InlineCacheKind::IncludePath,
        );
        table.install_include_path(
            15,
            function,
            block,
            instruction,
            request,
            InvalidationEpoch::new(2),
            IncludePathCacheTarget {
                canonical_path: PathBuf::from("/repo/first/lib.php"),
                fingerprint: IncludePathFileFingerprint {
                    len: 17,
                    modified_unix_nanos: Some(10),
                    readonly: false,
                },
            },
        );
        let (cached, event) = table.lookup_include_path(
            15,
            function,
            block,
            instruction,
            &changed,
            InvalidationEpoch::new(2),
        );

        assert!(cached.is_none());
        assert_eq!(event.kind, Some(InlineCacheKind::IncludePath));
        assert!(event.guard_failure);
        assert!(event.miss);
    }

    #[test]
    fn include_path_cache_invalidates_on_epoch_change() {
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);
        let mut table = InlineCacheTable::default();
        let request = IncludePathCacheKey {
            path: "lib.php".to_owned(),
            include_path: vec![PathBuf::from("src")],
            cwd: PathBuf::from("/repo"),
            calling_file_directory: Some(PathBuf::from("/repo/app")),
        };

        table.observe_slot(
            15,
            function,
            block,
            instruction,
            InlineCacheKind::IncludePath,
        );
        table.install_include_path(
            15,
            function,
            block,
            instruction,
            request.clone(),
            InvalidationEpoch::new(2),
            IncludePathCacheTarget {
                canonical_path: PathBuf::from("/repo/src/lib.php"),
                fingerprint: IncludePathFileFingerprint {
                    len: 17,
                    modified_unix_nanos: Some(10),
                    readonly: false,
                },
            },
        );
        let (cached, event) = table.lookup_include_path(
            15,
            function,
            block,
            instruction,
            &request,
            InvalidationEpoch::new(3),
        );

        assert!(cached.is_none());
        assert_eq!(event.kind, Some(InlineCacheKind::IncludePath));
        assert!(event.invalidation);
        assert!(event.miss);
    }
}
