//! Request-local inline-cache side table.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use php_runtime::api::PhpString;

use crate::include::{IncludeDirectoryVersion, IncludePathFileFingerprint};
use php_ir::ids::{BlockId, ClassId, FunctionId, InstrId};

mod method_peek;

/// Fixed guard-list size for experimental polymorphic method/property caches.
pub const POLYMORPHIC_INLINE_CACHE_LIMIT: usize = 4;
const NATIVE_CACHE_MEGAMORPHIC_GUARD_MISSES: u64 = 2;
const NATIVE_CACHE_DISABLE_GUARD_MISSES: u64 = 4;

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

    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
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
    ClassRelation,
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
            Self::ClassRelation => "class_relation",
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
}

/// VM-managed builtins resolved before generic user/internal function lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FunctionCallBuiltinKind {
    AutoloadOrSymbolIntrospection,
    Config,
    ErrorHandling,
    OutputBuffering,
    Environment,
    Process,
    FilterCallback,
    PcreCallback,
    ArrayCallback,
    ArraySort,
    InternalRegistry,
}

/// Resolution target cached by a function-call IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FunctionCallCacheTarget {
    CurrentUnit {
        unit_identity: u64,
        function: FunctionId,
    },
    DynamicUnit {
        unit_index: usize,
        /// Owning unit's cache identity: validates the request-local index
        /// across requests under worker-stable epochs and re-maps through
        /// the state's identity index when replay order shifted.
        unit_identity: u64,
        function: FunctionId,
    },
    Builtin {
        kind: FunctionCallBuiltinKind,
        name: Arc<str>,
    },
}

/// Guarded argument metadata for a function-call IC slot.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CallReferenceMask {
    inline: u64,
    overflow: Vec<u64>,
}

impl CallReferenceMask {
    #[must_use]
    pub fn from_flags(flags: impl IntoIterator<Item = bool>) -> Self {
        let mut mask = Self::default();
        for (index, is_reference) in flags.into_iter().enumerate() {
            if !is_reference {
                continue;
            }
            if index < u64::BITS as usize {
                mask.inline |= 1u64 << index;
                continue;
            }
            let overflow_index = index - u64::BITS as usize;
            let word = overflow_index / u64::BITS as usize;
            if mask.overflow.len() <= word {
                mask.overflow.resize(word + 1, 0);
            }
            mask.overflow[word] |= 1u64 << (overflow_index % u64::BITS as usize);
        }
        mask
    }

    #[must_use]
    pub fn any(&self) -> bool {
        self.inline != 0 || self.overflow.iter().any(|word| *word != 0)
    }
}

/// Guarded argument metadata for a function-call IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionCallShape {
    pub arity: u32,
    pub named_arguments: Vec<String>,
    pub by_ref_arguments: CallReferenceMask,
}

/// Persistable snapshot of one monomorphic entry-unit function-call IC site.
///
/// Only engine-owned, replay-stable metadata: the callsite coordinates and
/// target function are IR-derived (guarded by the feedback IR fingerprint),
/// the lowered name is an interned engine name, and the epoch records the
/// observation state. Dynamic-unit targets, builtins with implementation
/// metadata, named arguments, and by-reference shapes carry request-local or
/// broader guard state and are deliberately not persisted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionCallSiteSnapshot {
    pub function: u32,
    pub block: u32,
    pub instruction: u32,
    pub lowered_name: String,
    pub arity: u32,
    pub epoch: u64,
    pub target_function: u32,
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
    pub lowered_name: PhpString,
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
    pub by_ref_arguments: CallReferenceMask,
}

/// Stable method and receiver metadata guarded by a method-call IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodCallGuardMetadata {
    pub receiver_class_id: ClassId,
    pub class_layout_epoch: u64,
    pub method_table_epoch: u64,
    pub method_slot_index: Option<u32>,
    pub method_is_final: bool,
    pub method_is_private: bool,
    pub method_is_static: bool,
    pub receiver_has_override: bool,
    pub argument_shape: MethodCallShape,
    pub by_ref_compatible: bool,
    pub has_magic_call: bool,
}

/// Resolved method-call target payload kept out of native frame slots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodCallResolvedTarget {
    pub declaring_class: String,
    pub function: FunctionId,
    pub guard: MethodCallGuardMetadata,
    /// Execution route captured at fill time. The cache key already pins
    /// method, receiver class, calling scope, and lookup epoch, and the guard
    /// epochs invalidate the entry on class-table changes — so a hit may
    /// dispatch through this route without re-resolving the owner unit, the
    /// class entry, or the execution plan. `None` keeps the legacy
    /// re-resolving path.
    pub route: Option<MethodCallDispatchRoute>,
}

/// Owner unit, native entry, and declaring-class data a warmed method-call
/// site dispatches through directly.
#[derive(Clone, Debug)]
pub struct MethodCallDispatchRoute {
    /// Stable route identity. The owned unit and class entry below keep this
    /// identity alive, but are deliberately not used for equality.
    pub identity: MethodCallRouteIdentity,
    /// Unit whose IR owns the method body.
    pub owner: crate::compiled_unit::CompiledUnit,
    /// Published native generation holding the entry alive.
    pub native_generation: u64,
    /// Published native entry address.
    pub native_entry: usize,
    /// Declaring class entry, for trivial-method inlining on the hit path.
    pub declaring_class: crate::compiled_unit::CompiledClass,
    /// Normalized declaring-class name handle for frame class context.
    pub declaring_class_handle: std::sync::Arc<str>,
}

/// Allocation-independent identity for one resolved method body.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MethodCallRouteIdentity {
    pub owner_unit_identity: u64,
    pub declaring_class_id: ClassId,
    pub function: FunctionId,
    pub method_slot_index: u32,
}

impl PartialEq for MethodCallDispatchRoute {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl Eq for MethodCallDispatchRoute {}

/// Resolution target cached by a method-call IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MethodCallCacheTarget {
    CurrentUnit {
        target: Rc<MethodCallResolvedTarget>,
    },
    DynamicUnit {
        unit_index: usize,
        target: Rc<MethodCallResolvedTarget>,
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
    pub fn receiver_class_id(&self) -> ClassId {
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

/// Resolved property-fetch target payload kept out of native frame slots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyFetchResolvedTarget {
    pub receiver_class: String,
    pub declaring_class: String,
    pub property: String,
    pub storage_name: String,
    pub layout: PropertyFetchLayoutMetadata,
    /// Object-storage layout guard captured at install; `get_declared_slot`
    /// validates it so slot reads never observe a different class shape.
    pub object_layout_epoch: u64,
    /// Declared slot index under that layout, when the property is backed.
    pub declared_slot: Option<u32>,
}

/// Resolution target cached by a property-fetch IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PropertyFetchCacheTarget {
    CurrentUnit {
        target: Arc<PropertyFetchResolvedTarget>,
    },
    DynamicUnit {
        unit_index: usize,
        target: Arc<PropertyFetchResolvedTarget>,
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

/// Resolved property-assignment target payload kept out of native frame slots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyAssignResolvedTarget {
    pub receiver_class: String,
    pub declaring_class: String,
    pub property: String,
    pub storage_name: String,
    pub layout: PropertyAssignLayoutMetadata,
    /// Object-storage layout guard captured at install; `set_declared_slot`
    /// validates it so slot writes never touch a different class shape.
    pub object_layout_epoch: u64,
    /// Declared slot index under that layout, when the property is backed.
    pub declared_slot: Option<u32>,
    /// True when the slot write path is semantics-complete for this
    /// property: untyped, not readonly, no asymmetric set visibility.
    pub slot_write_eligible: bool,
}

/// Resolution target cached by a property-assignment IC slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PropertyAssignCacheTarget {
    CurrentUnit {
        target: Arc<PropertyAssignResolvedTarget>,
    },
    DynamicUnit {
        unit_index: usize,
        target: Arc<PropertyAssignResolvedTarget>,
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
    pub lowered_method: Arc<str>,
    pub receiver_class: Arc<str>,
    pub scope: Option<Arc<str>>,
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
    /// Candidate path re-canonicalized before hit acceptance so symlink swaps
    /// cannot keep returning an old canonical target.
    pub resolution_path: Option<PathBuf>,
    pub fingerprint: IncludePathFileFingerprint,
    /// Parent-directory version at resolve time, compared on revalidation for
    /// the `directory_version_*` counters only — never for hit acceptance.
    pub directory_version: Option<IncludeDirectoryVersion>,
}

/// Class-like lookup flavor cached by autoload lookup IC slots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutoloadClassLookupKind {
    ClassLike,
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
    /// Composer autoload-map fingerprint for the request. `Arc` so building
    /// one key per class-like lookup is a refcount bump, not a heap copy of
    /// the (request-constant) fingerprint string.
    pub composer_map_fingerprint: Option<std::sync::Arc<str>>,
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

/// Class/interface/trait/method relation cached by request-local relation slots.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ClassRelationKind {
    ExtendsClass,
    ImplementsInterface,
    TraitComposition,
    InstanceOf,
    MethodOverrideSlot,
    FinalMethodOrClass,
    VisibilityContext,
    AbstractInterfaceMethodRelation,
}

/// Stable request key for class, interface, trait, `instanceof`, and method
/// relation checks.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ClassRelationCacheKey {
    pub kind: ClassRelationKind,
    pub subject: String,
    pub target: String,
    pub member: Option<String>,
    pub visibility_context: Option<String>,
    pub config_fingerprint: String,
}

/// Epoch guards for relation checks that are sensitive to declaration loading,
/// autoload registration, trait/interface maps, and method-table layout.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClassRelationEpochs {
    pub class_table_epoch: u64,
    pub autoload_epoch: u64,
    pub include_eval_epoch: u64,
    pub trait_interface_map_version: u64,
    pub method_table_version: u64,
}

/// Cached boolean relation result plus optional resolved method metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassRelationCacheTarget {
    pub matches: bool,
    pub method_slot: Option<u32>,
    pub declaring_class: Option<String>,
}

/// One guarded class-relation cache entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassRelationCacheEntry {
    pub slot: InlineCacheId,
    pub key: ClassRelationCacheKey,
    pub epochs: ClassRelationEpochs,
    pub target: ClassRelationCacheTarget,
}

/// Lookup result for class-relation caches.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClassRelationCacheLookup {
    Hit(ClassRelationCacheTarget),
    Miss,
    Invalidated,
}

/// Worker-persistent class-relation cache. Keys include compiled-unit config
/// identity and every declaration/autoload epoch needed for safe reuse.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClassRelationCache {
    next_slot: u32,
    entries: BTreeMap<ClassRelationCacheKey, ClassRelationCacheEntry>,
}

impl ClassRelationCache {
    #[must_use]
    pub fn lookup(
        &mut self,
        key: &ClassRelationCacheKey,
        epochs: ClassRelationEpochs,
    ) -> ClassRelationCacheLookup {
        let Some(entry) = self.entries.get(key) else {
            return ClassRelationCacheLookup::Miss;
        };
        if entry.epochs == epochs {
            return ClassRelationCacheLookup::Hit(entry.target.clone());
        }
        self.entries.remove(key);
        ClassRelationCacheLookup::Invalidated
    }

    pub fn install(
        &mut self,
        key: ClassRelationCacheKey,
        epochs: ClassRelationEpochs,
        target: ClassRelationCacheTarget,
    ) -> InlineCacheId {
        let slot = self
            .entries
            .get(&key)
            .map(|entry| entry.slot)
            .unwrap_or_else(|| {
                let slot = InlineCacheId::new(self.next_slot);
                self.next_slot = self.next_slot.saturating_add(1);
                slot
            });
        self.entries.insert(
            key.clone(),
            ClassRelationCacheEntry {
                slot,
                key,
                epochs,
                target,
            },
        );
        slot
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Common metadata shared by every inline-cache family.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InlineCacheHeader {
    pub id: InlineCacheId,
    /// Entries were installed from persistent feedback (attribution only;
    /// every guard still validates at lookup).
    pub seeded: bool,
    /// The guarded payload existed before the current request began.
    pub persistent_worker: bool,
    pub state: InlineCacheState,
    pub unit_key: u64,
    pub function: FunctionId,
    pub block: BlockId,
    pub instruction: InstrId,
    pub epoch: InvalidationEpoch,
    pub stats: InlineCacheStats,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassStaticCacheEntry {
    pub kind: ClassConstantStaticPropertyCacheKind,
    pub resolved_class: String,
    pub member: String,
    pub scope: Option<String>,
    pub target: ClassConstantStaticPropertyCacheTarget,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncludePathCacheEntry {
    pub key: IncludePathCacheKey,
    pub target: IncludePathCacheTarget,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutoloadLookupCacheEntry {
    pub key: AutoloadClassLookupCacheKey,
    pub epochs: AutoloadClassLookupEpochs,
    pub target: AutoloadClassLookupCacheTarget,
}

/// Variant data for one inline-cache family. The family is derived from this
/// enum, so mismatched kind/payload combinations are not representable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InlineCachePayload {
    Empty(InlineCacheKind),
    FunctionCall(Vec<FunctionCallPolymorphicEntry>),
    MethodCall(Vec<MethodCallPolymorphicEntry>),
    PropertyFetch(Vec<PropertyFetchPolymorphicEntry>),
    PropertyAssign(Vec<PropertyAssignPolymorphicEntry>),
    ClassStatic(Vec<ClassStaticCacheEntry>),
    IncludePath(Vec<IncludePathCacheEntry>),
    AutoloadLookup(Vec<AutoloadLookupCacheEntry>),
}

impl InlineCachePayload {
    fn empty(kind: InlineCacheKind) -> Self {
        Self::Empty(kind)
    }

    #[must_use]
    pub const fn kind(&self) -> InlineCacheKind {
        match self {
            Self::Empty(kind) => *kind,
            Self::FunctionCall(_) => InlineCacheKind::FunctionCall,
            Self::MethodCall(_) => InlineCacheKind::MethodCall,
            Self::PropertyFetch(_) => InlineCacheKind::PropertyFetch,
            Self::PropertyAssign(_) => InlineCacheKind::PropertyAssign,
            Self::ClassStatic(_) => InlineCacheKind::ClassConstantStaticProperty,
            Self::IncludePath(_) => InlineCacheKind::IncludePath,
            Self::AutoloadLookup(_) => InlineCacheKind::AutoloadClassLookup,
        }
    }
}

/// One request-local inline-cache slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InlineCacheSlot {
    header: InlineCacheHeader,
    payload: InlineCachePayload,
}

impl InlineCacheSlot {
    #[must_use]
    pub const fn kind(&self) -> InlineCacheKind {
        self.payload.kind()
    }

    #[must_use]
    pub const fn header(&self) -> &InlineCacheHeader {
        &self.header
    }

    #[must_use]
    pub const fn payload(&self) -> &InlineCachePayload {
        &self.payload
    }

    fn function_call_entries(&self) -> &[FunctionCallPolymorphicEntry] {
        match &self.payload {
            InlineCachePayload::Empty(InlineCacheKind::FunctionCall) => &[],
            InlineCachePayload::FunctionCall(entries) => entries,
            _ => unreachable!("function-call operation on another cache family"),
        }
    }

    fn function_call_entries_mut(&mut self) -> &mut Vec<FunctionCallPolymorphicEntry> {
        if matches!(
            self.payload,
            InlineCachePayload::Empty(InlineCacheKind::FunctionCall)
        ) {
            self.payload = InlineCachePayload::FunctionCall(Vec::new());
        }
        match &mut self.payload {
            InlineCachePayload::FunctionCall(entries) => entries,
            _ => unreachable!("function-call operation on another cache family"),
        }
    }

    fn method_call_entries(&self) -> &[MethodCallPolymorphicEntry] {
        match &self.payload {
            InlineCachePayload::Empty(InlineCacheKind::MethodCall) => &[],
            InlineCachePayload::MethodCall(entries) => entries,
            _ => unreachable!("method-call operation on another cache family"),
        }
    }

    fn method_call_entries_mut(&mut self) -> &mut Vec<MethodCallPolymorphicEntry> {
        if matches!(
            self.payload,
            InlineCachePayload::Empty(InlineCacheKind::MethodCall)
        ) {
            self.payload = InlineCachePayload::MethodCall(Vec::new());
        }
        match &mut self.payload {
            InlineCachePayload::MethodCall(entries) => entries,
            _ => unreachable!("method-call operation on another cache family"),
        }
    }

    fn property_fetch_entries(&self) -> &[PropertyFetchPolymorphicEntry] {
        match &self.payload {
            InlineCachePayload::Empty(InlineCacheKind::PropertyFetch) => &[],
            InlineCachePayload::PropertyFetch(entries) => entries,
            _ => unreachable!("property-fetch operation on another cache family"),
        }
    }

    fn property_fetch_entries_mut(&mut self) -> &mut Vec<PropertyFetchPolymorphicEntry> {
        if matches!(
            self.payload,
            InlineCachePayload::Empty(InlineCacheKind::PropertyFetch)
        ) {
            self.payload = InlineCachePayload::PropertyFetch(Vec::new());
        }
        match &mut self.payload {
            InlineCachePayload::PropertyFetch(entries) => entries,
            _ => unreachable!("property-fetch operation on another cache family"),
        }
    }

    fn property_assign_entries(&self) -> &[PropertyAssignPolymorphicEntry] {
        match &self.payload {
            InlineCachePayload::Empty(InlineCacheKind::PropertyAssign) => &[],
            InlineCachePayload::PropertyAssign(entries) => entries,
            _ => unreachable!("property-assignment operation on another cache family"),
        }
    }

    fn property_assign_entries_mut(&mut self) -> &mut Vec<PropertyAssignPolymorphicEntry> {
        if matches!(
            self.payload,
            InlineCachePayload::Empty(InlineCacheKind::PropertyAssign)
        ) {
            self.payload = InlineCachePayload::PropertyAssign(Vec::new());
        }
        match &mut self.payload {
            InlineCachePayload::PropertyAssign(entries) => entries,
            _ => unreachable!("property-assignment operation on another cache family"),
        }
    }

    fn class_static_entries(&self) -> &[ClassStaticCacheEntry] {
        match &self.payload {
            InlineCachePayload::Empty(InlineCacheKind::ClassConstantStaticProperty) => &[],
            InlineCachePayload::ClassStatic(entries) => entries,
            _ => unreachable!("class-static operation on another cache family"),
        }
    }

    fn class_static_entries_mut(&mut self) -> &mut Vec<ClassStaticCacheEntry> {
        if matches!(
            self.payload,
            InlineCachePayload::Empty(InlineCacheKind::ClassConstantStaticProperty)
        ) {
            self.payload = InlineCachePayload::ClassStatic(Vec::new());
        }
        match &mut self.payload {
            InlineCachePayload::ClassStatic(entries) => entries,
            _ => unreachable!("class-static operation on another cache family"),
        }
    }

    fn include_path_entries(&self) -> &[IncludePathCacheEntry] {
        match &self.payload {
            InlineCachePayload::Empty(InlineCacheKind::IncludePath) => &[],
            InlineCachePayload::IncludePath(entries) => entries,
            _ => unreachable!("include-path operation on another cache family"),
        }
    }

    fn include_path_entries_mut(&mut self) -> &mut Vec<IncludePathCacheEntry> {
        if matches!(
            self.payload,
            InlineCachePayload::Empty(InlineCacheKind::IncludePath)
        ) {
            self.payload = InlineCachePayload::IncludePath(Vec::new());
        }
        match &mut self.payload {
            InlineCachePayload::IncludePath(entries) => entries,
            _ => unreachable!("include-path operation on another cache family"),
        }
    }

    fn autoload_lookup_entries(&self) -> &[AutoloadLookupCacheEntry] {
        match &self.payload {
            InlineCachePayload::Empty(InlineCacheKind::AutoloadClassLookup) => &[],
            InlineCachePayload::AutoloadLookup(entries) => entries,
            _ => unreachable!("autoload operation on another cache family"),
        }
    }

    fn autoload_lookup_entries_mut(&mut self) -> &mut Vec<AutoloadLookupCacheEntry> {
        if matches!(
            self.payload,
            InlineCachePayload::Empty(InlineCacheKind::AutoloadClassLookup)
        ) {
            self.payload = InlineCachePayload::AutoloadLookup(Vec::new());
        }
        match &mut self.payload {
            InlineCachePayload::AutoloadLookup(entries) => entries,
            _ => unreachable!("autoload operation on another cache family"),
        }
    }
}

impl std::ops::Deref for InlineCacheSlot {
    type Target = InlineCacheHeader;

    fn deref(&self) -> &Self::Target {
        &self.header
    }
}

impl std::ops::DerefMut for InlineCacheSlot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.header
    }
}

/// Result of observing one candidate instruction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InlineCacheObservation {
    pub candidate: bool,
    /// The slot's current entries were installed from persistent feedback.
    pub seeded: bool,
    pub persistent_worker: bool,
    pub slot_allocated: bool,
    pub kind: Option<InlineCacheKind>,
    pub hit: bool,
    pub miss: bool,
    pub invalidation: bool,
    pub guard_failure: bool,
    pub resolver_required: bool,
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
            resolver_required: true,
            ..Self::empty()
        }
    }

    #[must_use]
    pub const fn invalidation() -> Self {
        Self {
            miss: true,
            invalidation: true,
            resolver_required: true,
            ..Self::empty()
        }
    }

    #[must_use]
    pub const fn guard_failure() -> Self {
        Self {
            miss: true,
            guard_failure: true,
            resolver_required: true,
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
            resolver_required: true,
            disabled: true,
            ..Self::empty()
        }
    }

    #[must_use]
    pub const fn empty() -> Self {
        Self {
            candidate: false,
            seeded: false,
            persistent_worker: false,
            slot_allocated: false,
            kind: None,
            hit: false,
            miss: false,
            invalidation: false,
            guard_failure: false,
            resolver_required: false,
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
    InlineCacheObservation {
        seeded: slot.seeded,
        persistent_worker: slot.persistent_worker,
        monomorphic: slot.state == InlineCacheState::Monomorphic,
        polymorphic: slot.state == InlineCacheState::Polymorphic,
        ..InlineCacheObservation::hit()
    }
}

fn record_slot_miss(slot: &mut InlineCacheSlot) -> InlineCacheObservation {
    slot.stats.misses = slot.stats.misses.saturating_add(1);
    InlineCacheObservation {
        persistent_worker: slot.persistent_worker,
        ..InlineCacheObservation::miss()
    }
}

fn record_slot_invalidation(slot: &mut InlineCacheSlot) -> InlineCacheObservation {
    slot.stats.invalidations = slot.stats.invalidations.saturating_add(1);
    slot.stats.misses = slot.stats.misses.saturating_add(1);
    slot.state = InlineCacheState::Cold;
    let seeded = slot.seeded;
    let persistent_worker = slot.persistent_worker;
    slot.seeded = false;
    clear_slot_targets(slot);
    InlineCacheObservation {
        seeded,
        persistent_worker,
        ..InlineCacheObservation::invalidation()
    }
}

fn record_slot_guard_failure(slot: &mut InlineCacheSlot) -> InlineCacheObservation {
    slot.stats.guard_failures = slot.stats.guard_failures.saturating_add(1);
    slot.stats.misses = slot.stats.misses.saturating_add(1);
    let mut observation = InlineCacheObservation {
        guard_failure: true,
        resolver_required: true,
        miss: true,
        ..InlineCacheObservation::empty()
    };

    if slot.stats.guard_failures >= NATIVE_CACHE_DISABLE_GUARD_MISSES {
        slot.stats.disabled_transitions = slot.stats.disabled_transitions.saturating_add(1);
        slot.state = InlineCacheState::Disabled;
        clear_slot_targets(slot);
        observation.disabled = true;
    } else if slot.stats.guard_failures >= NATIVE_CACHE_MEGAMORPHIC_GUARD_MISSES
        && slot.stats.megamorphic_transitions == 0
    {
        slot.stats.megamorphic_transitions = slot.stats.megamorphic_transitions.saturating_add(1);
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
        resolver_required: true,
        megamorphic: true,
        ..InlineCacheObservation::empty()
    }
}

fn mark_slot_megamorphic(slot: &mut InlineCacheSlot) {
    if slot.state == InlineCacheState::Megamorphic {
        return;
    }
    slot.stats.megamorphic_transitions = slot.stats.megamorphic_transitions.saturating_add(1);
    slot.state = InlineCacheState::Megamorphic;
    clear_slot_targets(slot);
}

fn clear_slot_targets(slot: &mut InlineCacheSlot) {
    let kind = slot.payload.kind();
    slot.payload = InlineCachePayload::empty(kind);
    slot.persistent_worker = false;
}

fn inline_cache_payload_len(payload: &InlineCachePayload) -> usize {
    match payload {
        InlineCachePayload::FunctionCall(entries) => entries.len(),
        InlineCachePayload::MethodCall(entries) => entries.len(),
        InlineCachePayload::PropertyFetch(entries) => entries.len(),
        InlineCachePayload::PropertyAssign(entries) => entries.len(),
        InlineCachePayload::ClassStatic(entries) => entries.len(),
        InlineCachePayload::IncludePath(entries) => entries.len(),
        InlineCachePayload::AutoloadLookup(entries) => entries.len(),
        InlineCachePayload::Empty(_) => 0,
    }
}

fn finish_polymorphic_install(
    slot: &mut InlineCacheSlot,
    epoch: InvalidationEpoch,
    entry_count: usize,
) {
    debug_assert!(entry_count > 0);
    slot.state = if entry_count == 1 {
        InlineCacheState::Monomorphic
    } else {
        InlineCacheState::Polymorphic
    };
    slot.epoch = epoch;
    slot.seeded = false;
    slot.persistent_worker = false;
}

fn sync_function_call_primary(slot: &mut InlineCacheSlot) {
    let Some(first) = slot.function_call_entries().first().cloned() else {
        slot.state = InlineCacheState::Cold;
        return;
    };
    slot.state = if slot.function_call_entries().len() == 1 {
        InlineCacheState::Monomorphic
    } else {
        InlineCacheState::Polymorphic
    };
    slot.epoch = first.epoch;
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

#[derive(Clone, Copy)]
struct ClassStaticGuard<'a> {
    kind: ClassConstantStaticPropertyCacheKind,
    resolved_class: &'a str,
    member: &'a str,
    scope: Option<&'a str>,
}

fn class_static_guard_matches(guard: ClassStaticGuard<'_>, cached: &ClassStaticCacheEntry) -> bool {
    cached.kind == guard.kind
        && cached.resolved_class == guard.resolved_class
        && cached.member == guard.member
        && cached
            .scope
            .as_deref()
            .is_none_or(|scope| Some(scope) == guard.scope)
}

/// Per-request inline-cache metadata table.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InlineCacheTable {
    next_id: u32,
    site_ids: BTreeMap<InlineCacheKey, InlineCacheId>,
    slots: Vec<InlineCacheSlot>,
}

impl InlineCacheTable {
    /// Marks guarded payloads that survived from an earlier request. Empty
    /// slots remain ordinary cold misses and are not attributed as reuse.
    pub fn begin_request(&mut self, retain_static: bool) -> usize {
        let mut dynamic_invalidations = 0usize;
        for slot in &mut self.slots {
            if !retain_static {
                let removed = inline_cache_payload_len(&slot.payload);
                dynamic_invalidations = dynamic_invalidations.saturating_add(removed);
                clear_slot_targets(slot);
                slot.state = InlineCacheState::Cold;
                slot.persistent_worker = false;
                continue;
            }
            let mut removed = 0usize;
            let retained = match &mut slot.payload {
                InlineCachePayload::FunctionCall(entries) => {
                    let before = entries.len();
                    entries.retain(|entry| {
                        matches!(entry.target, FunctionCallCacheTarget::CurrentUnit { .. })
                    });
                    removed = before.saturating_sub(entries.len());
                    !entries.is_empty()
                }
                InlineCachePayload::MethodCall(entries) => {
                    let before = entries.len();
                    entries.retain(|entry| {
                        matches!(entry.target, MethodCallCacheTarget::CurrentUnit { .. })
                    });
                    removed = before.saturating_sub(entries.len());
                    !entries.is_empty()
                }
                InlineCachePayload::PropertyFetch(entries) => {
                    let before = entries.len();
                    entries.retain(|entry| {
                        matches!(entry.target, PropertyFetchCacheTarget::CurrentUnit { .. })
                    });
                    removed = before.saturating_sub(entries.len());
                    !entries.is_empty()
                }
                InlineCachePayload::PropertyAssign(entries) => {
                    let before = entries.len();
                    entries.retain(|entry| {
                        matches!(entry.target, PropertyAssignCacheTarget::CurrentUnit { .. })
                    });
                    removed = before.saturating_sub(entries.len());
                    !entries.is_empty()
                }
                InlineCachePayload::ClassStatic(entries) => {
                    let before = entries.len();
                    entries.retain(|entry| {
                        matches!(
                            entry.target,
                            ClassConstantStaticPropertyCacheTarget::CurrentUnit { .. }
                        )
                    });
                    removed = before.saturating_sub(entries.len());
                    !entries.is_empty()
                }
                InlineCachePayload::Empty(_) => false,
                InlineCachePayload::IncludePath(_) | InlineCachePayload::AutoloadLookup(_) => true,
            };
            dynamic_invalidations = dynamic_invalidations.saturating_add(removed);
            if !retained && removed > 0 {
                clear_slot_targets(slot);
                slot.state = InlineCacheState::Cold;
            } else if removed > 0 {
                let entry_count = match &slot.payload {
                    InlineCachePayload::FunctionCall(entries) => entries.len(),
                    InlineCachePayload::MethodCall(entries) => entries.len(),
                    InlineCachePayload::PropertyFetch(entries) => entries.len(),
                    InlineCachePayload::PropertyAssign(entries) => entries.len(),
                    InlineCachePayload::ClassStatic(entries) => entries.len(),
                    InlineCachePayload::IncludePath(entries) => entries.len(),
                    InlineCachePayload::AutoloadLookup(entries) => entries.len(),
                    InlineCachePayload::Empty(_) => 0,
                };
                slot.state = if entry_count == 1 {
                    InlineCacheState::Monomorphic
                } else {
                    InlineCacheState::Polymorphic
                };
            }
            slot.persistent_worker = !matches!(slot.payload, InlineCachePayload::Empty(_));
        }
        dynamic_invalidations
    }

    /// Allocates or finds the dense ID for one inline-cache candidate.
    pub fn bind_slot(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        kind: InlineCacheKind,
    ) -> (InlineCacheId, InlineCacheObservation) {
        let key = inline_cache_key(unit_key, function, block, instruction, kind);
        if let Some(id) = self.site_ids.get(&key).copied() {
            return (
                id,
                InlineCacheObservation {
                    candidate: true,
                    kind: Some(kind),
                    ..InlineCacheObservation::empty()
                },
            );
        }

        let id = InlineCacheId::new(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.site_ids.insert(key, id);
        self.slots.push(InlineCacheSlot {
            header: InlineCacheHeader {
                id,
                seeded: false,
                persistent_worker: false,
                state: InlineCacheState::Cold,
                unit_key,
                function,
                block,
                instruction,
                epoch: InvalidationEpoch::default(),
                stats: InlineCacheStats::default(),
            },
            payload: InlineCachePayload::empty(kind),
        });
        (
            id,
            InlineCacheObservation {
                candidate: true,
                slot_allocated: true,
                kind: Some(kind),
                ..InlineCacheObservation::empty()
            },
        )
    }

    /// Allocates or finds the slot for one inline-cache candidate.
    pub fn observe_slot(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        kind: InlineCacheKind,
    ) -> InlineCacheObservation {
        self.bind_slot(unit_key, function, block, instruction, kind)
            .1
    }

    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    fn slot(&self, key: &InlineCacheKey) -> Option<&InlineCacheSlot> {
        let id = *self.site_ids.get(key)?;
        self.slots.get(id.index())
    }

    fn slot_mut(&mut self, key: &InlineCacheKey) -> Option<&mut InlineCacheSlot> {
        let id = *self.site_ids.get(key)?;
        self.slots.get_mut(id.index())
    }

    fn slot_by_id(&self, id: InlineCacheId) -> Option<&InlineCacheSlot> {
        self.slots.get(id.index())
    }

    fn slot_by_id_mut(&mut self, id: InlineCacheId) -> Option<&mut InlineCacheSlot> {
        self.slots.get_mut(id.index())
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
        self.slot(&key)?
            .function_call_entries()
            .first()
            .map(|entry| entry.target.clone())
    }

    #[must_use]
    pub fn peek_function_call_builtin_metadata(
        &self,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        lowered_name: &PhpString,
        shape: &FunctionCallShape,
    ) -> Option<FunctionCallBuiltinMetadata> {
        let key = inline_cache_key(
            unit_key,
            function,
            block,
            instruction,
            InlineCacheKind::FunctionCall,
        );
        let slot = self.slot(&key)?;
        Self::peek_function_call_builtin_metadata_in_slot(slot, lowered_name, shape)
    }

    #[must_use]
    pub fn peek_function_call_builtin_metadata_by_id(
        &self,
        id: InlineCacheId,
        lowered_name: &PhpString,
        shape: &FunctionCallShape,
    ) -> Option<FunctionCallBuiltinMetadata> {
        let slot = self.slot_by_id(id)?;
        if slot.kind() != InlineCacheKind::FunctionCall {
            return None;
        }
        Self::peek_function_call_builtin_metadata_in_slot(slot, lowered_name, shape)
    }

    fn peek_function_call_builtin_metadata_in_slot(
        slot: &InlineCacheSlot,
        lowered_name: &PhpString,
        shape: &FunctionCallShape,
    ) -> Option<FunctionCallBuiltinMetadata> {
        slot.function_call_entries()
            .iter()
            .find(|entry| entry.lowered_name == *lowered_name && entry.shape == *shape)
            .and_then(|entry| entry.builtin_metadata.clone())
    }

    /// Exports the entry unit's monomorphic function-call sites in the
    /// persistable subset (see [`FunctionCallSiteSnapshot`]) for persistent
    /// feedback. Deterministically ordered by callsite coordinates.
    #[must_use]
    pub fn export_persistent_function_callsites(
        &self,
        entry_unit_key: u64,
    ) -> Vec<FunctionCallSiteSnapshot> {
        let mut sites: Vec<FunctionCallSiteSnapshot> = self
            .slots
            .iter()
            .filter_map(|slot| {
                if slot.kind() != InlineCacheKind::FunctionCall
                    || slot.state != InlineCacheState::Monomorphic
                    || slot.unit_key != entry_unit_key
                {
                    return None;
                }
                let [entry] = slot.function_call_entries() else {
                    return None;
                };
                let name = &entry.lowered_name;
                let shape = &entry.shape;
                let builtin_metadata = entry.builtin_metadata.as_ref();
                let target = &entry.target;
                if builtin_metadata.is_some()
                    || !shape.named_arguments.is_empty()
                    || shape.by_ref_arguments.any()
                {
                    return None;
                }
                let FunctionCallCacheTarget::CurrentUnit { function, .. } = target else {
                    return None;
                };
                Some(FunctionCallSiteSnapshot {
                    function: slot.function.raw(),
                    block: slot.block.raw(),
                    instruction: slot.instruction.raw(),
                    lowered_name: name.to_string(),
                    arity: shape.arity,
                    epoch: entry.epoch.raw(),
                    target_function: function.raw(),
                })
            })
            .collect();
        sites.sort_by_key(|site| (site.function, site.block, site.instruction));
        sites
    }

    /// Seeds monomorphic function-call IC sites exported by a prior run.
    ///
    /// `target_resolves` re-derives the seed's soundness against the current
    /// entry unit: it must return true only when the recorded target function
    /// exists and is the one the recorded call name resolves to *now*. This
    /// closes the gap that the lookup guard cannot — the guard matches
    /// name/arity/epoch but never re-resolves name→target, so a seed whose
    /// recorded target no longer matches the name (a namespace-fallback call
    /// whose namespaced definition now exists, a tampered target id, or an
    /// out-of-range id) would otherwise dispatch the wrong function. Rejected
    /// seeds never create a slot or intern a name.
    ///
    /// Seeded entries that pass still run behind the **full lookup guard
    /// protocol**: name, arity shape, and observation epoch must all match at
    /// the callsite. Already-touched slots are skipped; the returned count
    /// reflects seeds that took effect.
    pub fn seed_persistent_function_callsites(
        &mut self,
        entry_unit_key: u64,
        sites: &[FunctionCallSiteSnapshot],
        target_resolves: impl Fn(&FunctionCallSiteSnapshot) -> bool,
    ) -> usize {
        let mut seeded = 0usize;
        for site in sites {
            if !target_resolves(site) {
                continue;
            }
            let function = FunctionId::new(site.function);
            let block = BlockId::new(site.block);
            let instruction = InstrId::new(site.instruction);
            let key = inline_cache_key(
                entry_unit_key,
                function,
                block,
                instruction,
                InlineCacheKind::FunctionCall,
            );
            if self.site_ids.contains_key(&key) {
                continue;
            }
            self.observe_slot(
                entry_unit_key,
                function,
                block,
                instruction,
                InlineCacheKind::FunctionCall,
            );
            self.install_function_call(
                entry_unit_key,
                function,
                block,
                instruction,
                &PhpString::intern(site.lowered_name.as_bytes()),
                InvalidationEpoch::new(site.epoch),
                FunctionCallShape {
                    arity: site.arity,
                    named_arguments: Vec::new(),
                    by_ref_arguments: CallReferenceMask::default(),
                },
                None,
                FunctionCallCacheTarget::CurrentUnit {
                    unit_identity: entry_unit_key,
                    function: FunctionId::new(site.target_function),
                },
            );
            if let Some(slot) = self.slot_mut(&key) {
                slot.seeded = true;
                seeded += 1;
            }
        }
        seeded
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
        lowered_name: &PhpString,
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
        let Some(id) = self.site_ids.get(&key).copied() else {
            return (
                None,
                with_kind(
                    InlineCacheKind::FunctionCall,
                    InlineCacheObservation::miss(),
                ),
            );
        };
        self.lookup_function_call_by_id(id, lowered_name, epoch, shape, builtin_metadata)
    }

    pub fn lookup_function_call_by_id(
        &mut self,
        id: InlineCacheId,
        lowered_name: &PhpString,
        epoch: InvalidationEpoch,
        shape: &FunctionCallShape,
        builtin_metadata: Option<&FunctionCallBuiltinMetadata>,
    ) -> (Option<FunctionCallCacheTarget>, InlineCacheObservation) {
        let Some(slot) = self.slot_by_id_mut(id) else {
            return (
                None,
                with_kind(
                    InlineCacheKind::FunctionCall,
                    InlineCacheObservation::miss(),
                ),
            );
        };
        if slot.kind() != InlineCacheKind::FunctionCall {
            return (
                None,
                with_kind(
                    InlineCacheKind::FunctionCall,
                    InlineCacheObservation::miss(),
                ),
            );
        }
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
        if !slot.function_call_entries().is_empty() {
            if let Some(index) = slot.function_call_entries().iter().position(|entry| {
                entry.lowered_name == *lowered_name
                    && entry.shape == *shape
                    && entry.builtin_metadata.as_ref() == builtin_metadata
            }) {
                let entry_epoch = slot.function_call_entries()[index].epoch;
                if entry_epoch != epoch {
                    let observation = record_slot_invalidation(slot);
                    return (None, with_kind(InlineCacheKind::FunctionCall, observation));
                }
                let target = slot.function_call_entries()[index].target.clone();
                let observation = record_slot_hit(slot);
                return (
                    Some(target),
                    with_kind(InlineCacheKind::FunctionCall, observation),
                );
            }
            let same_name = slot
                .function_call_entries()
                .iter()
                .any(|entry| entry.lowered_name == *lowered_name);
            let same_name_and_shape = slot
                .function_call_entries()
                .iter()
                .any(|entry| entry.lowered_name == *lowered_name && entry.shape == *shape);
            let observation = if same_name {
                record_slot_guard_failure(slot)
            } else if slot.function_call_entries().len() < POLYMORPHIC_INLINE_CACHE_LIMIT {
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
        let observation = record_slot_miss(slot);
        (None, with_kind(InlineCacheKind::FunctionCall, observation))
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
        lowered_name: &PhpString,
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
        if let Some(id) = self.site_ids.get(&key).copied() {
            self.install_function_call_by_id(
                id,
                lowered_name,
                epoch,
                shape,
                builtin_metadata,
                target,
            );
        }
    }

    pub fn install_function_call_by_id(
        &mut self,
        id: InlineCacheId,
        lowered_name: &PhpString,
        epoch: InvalidationEpoch,
        shape: FunctionCallShape,
        builtin_metadata: Option<FunctionCallBuiltinMetadata>,
        target: FunctionCallCacheTarget,
    ) {
        if let Some(slot) = self.slot_by_id_mut(id) {
            if slot.kind() != InlineCacheKind::FunctionCall {
                return;
            }
            if matches!(
                slot.state,
                InlineCacheState::Disabled | InlineCacheState::Megamorphic
            ) {
                return;
            }
            // A runtime install means the slot is no longer purely
            // seed-derived, so drop seed attribution; the seeder re-sets
            // `seeded` after its own install call, keeping the flag true only
            // for slots touched exclusively by seeding.
            slot.seeded = false;
            slot.persistent_worker = false;
            let new_entry = FunctionCallPolymorphicEntry {
                lowered_name: lowered_name.clone(),
                epoch,
                shape,
                builtin_metadata,
                target,
            };
            if let Some(index) = slot.function_call_entries().iter().position(|entry| {
                entry.lowered_name == new_entry.lowered_name
                    && entry.shape == new_entry.shape
                    && entry.builtin_metadata == new_entry.builtin_metadata
            }) {
                slot.function_call_entries_mut()[index] = new_entry;
            } else {
                if slot.function_call_entries().len() >= POLYMORPHIC_INLINE_CACHE_LIMIT {
                    mark_slot_megamorphic(slot);
                    return;
                }
                slot.function_call_entries_mut().push(new_entry);
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
        let Some(id) = self.site_ids.get(&key).copied() else {
            return (
                None,
                with_kind(InlineCacheKind::MethodCall, InlineCacheObservation::miss()),
            );
        };
        self.lookup_method_call_by_id(id, lowered_method, receiver_class, scope, epoch)
    }

    pub fn lookup_method_call_by_id(
        &mut self,
        id: InlineCacheId,
        lowered_method: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> (Option<MethodCallCacheTarget>, InlineCacheObservation) {
        let Some(slot) = self.slot_by_id_mut(id) else {
            return (
                None,
                with_kind(InlineCacheKind::MethodCall, InlineCacheObservation::miss()),
            );
        };
        if slot.kind() != InlineCacheKind::MethodCall {
            return (
                None,
                with_kind(InlineCacheKind::MethodCall, InlineCacheObservation::miss()),
            );
        }
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
        if !slot.method_call_entries().is_empty() {
            if let Some(index) = slot.method_call_entries().iter().position(|entry| {
                method_guard_matches(
                    lowered_method,
                    &entry.lowered_method,
                    receiver_class,
                    &entry.receiver_class,
                    scope,
                    entry.scope.as_deref(),
                )
            }) {
                let entry_epoch = slot.method_call_entries()[index].epoch;
                if entry_epoch != epoch {
                    let observation = record_slot_invalidation(slot);
                    return (None, with_kind(InlineCacheKind::MethodCall, observation));
                }
                let target = slot.method_call_entries()[index].target.clone();
                let observation = record_slot_hit(slot);
                return (
                    Some(target),
                    with_kind(InlineCacheKind::MethodCall, observation),
                );
            }
            let same_method_and_scope = slot.method_call_entries().iter().any(|entry| {
                entry.lowered_method.as_ref() == lowered_method && entry.scope.as_deref() == scope
            });
            let observation = if slot.method_call_entries().len() > 1
                && same_method_and_scope
                && slot.method_call_entries().len() < POLYMORPHIC_INLINE_CACHE_LIMIT
            {
                record_slot_miss(slot)
            } else {
                record_slot_guard_failure(slot)
            };
            return (None, with_kind(InlineCacheKind::MethodCall, observation));
        }
        let observation = record_slot_miss(slot);
        (None, with_kind(InlineCacheKind::MethodCall, observation))
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
        if let Some(id) = self.site_ids.get(&key).copied() {
            self.install_method_call_by_id(
                id,
                lowered_method,
                receiver_class,
                scope,
                epoch,
                target,
            );
        }
    }

    pub fn install_method_call_by_id(
        &mut self,
        id: InlineCacheId,
        lowered_method: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
        target: MethodCallCacheTarget,
    ) {
        if let Some(slot) = self.slot_by_id_mut(id) {
            if slot.kind() != InlineCacheKind::MethodCall {
                return;
            }
            if matches!(
                slot.state,
                InlineCacheState::Disabled | InlineCacheState::Megamorphic
            ) {
                return;
            }
            let new_entry = MethodCallPolymorphicEntry {
                lowered_method: Arc::from(lowered_method),
                receiver_class: Arc::from(receiver_class),
                scope: scope.map(Arc::from),
                epoch,
                target,
            };
            if let Some(index) = slot.method_call_entries().iter().position(|entry| {
                method_guard_matches(
                    lowered_method,
                    &entry.lowered_method,
                    receiver_class,
                    &entry.receiver_class,
                    scope,
                    entry.scope.as_deref(),
                )
            }) {
                slot.method_call_entries_mut()[index] = new_entry;
            } else {
                if slot.method_call_entries().len() >= POLYMORPHIC_INLINE_CACHE_LIMIT {
                    mark_slot_megamorphic(slot);
                    return;
                }
                slot.method_call_entries_mut().push(new_entry);
            }
            slot.state = if slot.method_call_entries().len() == 1 {
                InlineCacheState::Monomorphic
            } else {
                InlineCacheState::Polymorphic
            };
            slot.epoch = slot.method_call_entries()[0].epoch;
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
        let Some(id) = self.site_ids.get(&key).copied() else {
            return (
                None,
                with_kind(
                    InlineCacheKind::PropertyFetch,
                    InlineCacheObservation::miss(),
                ),
            );
        };
        self.lookup_property_fetch_by_id(id, property, receiver_class, scope, epoch)
    }

    pub fn lookup_property_fetch_by_id(
        &mut self,
        id: InlineCacheId,
        property: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> (Option<PropertyFetchCacheTarget>, InlineCacheObservation) {
        let Some(slot) = self.slot_by_id_mut(id) else {
            return (
                None,
                with_kind(
                    InlineCacheKind::PropertyFetch,
                    InlineCacheObservation::miss(),
                ),
            );
        };
        if slot.kind() != InlineCacheKind::PropertyFetch {
            return (
                None,
                with_kind(
                    InlineCacheKind::PropertyFetch,
                    InlineCacheObservation::miss(),
                ),
            );
        }
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
        if !slot.property_fetch_entries().is_empty() {
            if let Some(index) = slot.property_fetch_entries().iter().position(|entry| {
                property_guard_matches(
                    property,
                    &entry.property,
                    receiver_class,
                    &entry.receiver_class,
                    scope,
                    entry.scope.as_deref(),
                )
            }) {
                let entry_epoch = slot.property_fetch_entries()[index].epoch;
                if entry_epoch != epoch {
                    let observation = record_slot_invalidation(slot);
                    return (None, with_kind(InlineCacheKind::PropertyFetch, observation));
                }
                let target = slot.property_fetch_entries()[index].target.clone();
                let observation = record_slot_hit(slot);
                return (
                    Some(target),
                    with_kind(InlineCacheKind::PropertyFetch, observation),
                );
            }
            let same_property_and_scope = slot.property_fetch_entries().iter().any(|entry| {
                entry.property == property && property_scope_matches(entry.scope.as_deref(), scope)
            });
            let observation = if slot.property_fetch_entries().len() > 1
                && same_property_and_scope
                && slot.property_fetch_entries().len() < POLYMORPHIC_INLINE_CACHE_LIMIT
            {
                record_slot_miss(slot)
            } else {
                record_slot_guard_failure(slot)
            };
            return (None, with_kind(InlineCacheKind::PropertyFetch, observation));
        }
        let observation = record_slot_miss(slot);
        (None, with_kind(InlineCacheKind::PropertyFetch, observation))
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
        if let Some(id) = self.site_ids.get(&key).copied() {
            self.install_property_fetch_by_id(id, property, receiver_class, scope, epoch, target);
        }
    }

    pub fn install_property_fetch_by_id(
        &mut self,
        id: InlineCacheId,
        property: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
        target: PropertyFetchCacheTarget,
    ) {
        if let Some(slot) = self.slot_by_id_mut(id) {
            if slot.kind() != InlineCacheKind::PropertyFetch {
                return;
            }
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
            if let Some(index) = slot.property_fetch_entries().iter().position(|entry| {
                property_guard_matches(
                    property,
                    &entry.property,
                    receiver_class,
                    &entry.receiver_class,
                    scope,
                    entry.scope.as_deref(),
                )
            }) {
                slot.property_fetch_entries_mut()[index] = new_entry;
            } else {
                if slot.property_fetch_entries().len() >= POLYMORPHIC_INLINE_CACHE_LIMIT {
                    mark_slot_megamorphic(slot);
                    return;
                }
                slot.property_fetch_entries_mut().push(new_entry);
            }
            slot.state = if slot.property_fetch_entries().len() == 1 {
                InlineCacheState::Monomorphic
            } else {
                InlineCacheState::Polymorphic
            };
            slot.epoch = slot.property_fetch_entries()[0].epoch;
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
        let Some(id) = self.site_ids.get(&key).copied() else {
            return (
                None,
                with_kind(
                    InlineCacheKind::PropertyAssign,
                    InlineCacheObservation::miss(),
                ),
            );
        };
        self.lookup_property_assign_by_id(id, property, receiver_class, scope, epoch)
    }

    pub fn lookup_property_assign_by_id(
        &mut self,
        id: InlineCacheId,
        property: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> (Option<PropertyAssignCacheTarget>, InlineCacheObservation) {
        let Some(slot) = self.slot_by_id_mut(id) else {
            return (
                None,
                with_kind(
                    InlineCacheKind::PropertyAssign,
                    InlineCacheObservation::miss(),
                ),
            );
        };
        if slot.kind() != InlineCacheKind::PropertyAssign {
            return (
                None,
                with_kind(
                    InlineCacheKind::PropertyAssign,
                    InlineCacheObservation::miss(),
                ),
            );
        }
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
        if !slot.property_assign_entries().is_empty() {
            if let Some(index) = slot.property_assign_entries().iter().position(|entry| {
                property_guard_matches(
                    property,
                    &entry.property,
                    receiver_class,
                    &entry.receiver_class,
                    scope,
                    entry.scope.as_deref(),
                )
            }) {
                let entry_epoch = slot.property_assign_entries()[index].epoch;
                if entry_epoch != epoch {
                    let observation = record_slot_invalidation(slot);
                    return (
                        None,
                        with_kind(InlineCacheKind::PropertyAssign, observation),
                    );
                }
                let target = slot.property_assign_entries()[index].target.clone();
                let observation = record_slot_hit(slot);
                return (
                    Some(target),
                    with_kind(InlineCacheKind::PropertyAssign, observation),
                );
            }
            let same_property_and_scope = slot.property_assign_entries().iter().any(|entry| {
                entry.property == property && property_scope_matches(entry.scope.as_deref(), scope)
            });
            let observation = if slot.property_assign_entries().len() > 1
                && same_property_and_scope
                && slot.property_assign_entries().len() < POLYMORPHIC_INLINE_CACHE_LIMIT
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
        let observation = record_slot_miss(slot);
        (
            None,
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
        if let Some(id) = self.site_ids.get(&key).copied() {
            self.install_property_assign_by_id(id, property, receiver_class, scope, epoch, target);
        }
    }

    pub fn install_property_assign_by_id(
        &mut self,
        id: InlineCacheId,
        property: &str,
        receiver_class: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
        target: PropertyAssignCacheTarget,
    ) {
        if let Some(slot) = self.slot_by_id_mut(id) {
            if slot.kind() != InlineCacheKind::PropertyAssign {
                return;
            }
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
            if let Some(index) = slot.property_assign_entries().iter().position(|entry| {
                property_guard_matches(
                    property,
                    &entry.property,
                    receiver_class,
                    &entry.receiver_class,
                    scope,
                    entry.scope.as_deref(),
                )
            }) {
                slot.property_assign_entries_mut()[index] = new_entry;
            } else {
                if slot.property_assign_entries().len() >= POLYMORPHIC_INLINE_CACHE_LIMIT {
                    mark_slot_megamorphic(slot);
                    return;
                }
                slot.property_assign_entries_mut().push(new_entry);
            }
            slot.state = if slot.property_assign_entries().len() == 1 {
                InlineCacheState::Monomorphic
            } else {
                InlineCacheState::Polymorphic
            };
            slot.epoch = slot.property_assign_entries()[0].epoch;
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
        let Some(id) = self.site_ids.get(&key).copied() else {
            return (
                None,
                with_kind(
                    InlineCacheKind::ClassConstantStaticProperty,
                    InlineCacheObservation::miss(),
                ),
            );
        };
        self.lookup_class_constant_static_property_by_id(
            id,
            kind,
            resolved_class,
            member,
            scope,
            epoch,
        )
    }

    pub fn lookup_class_constant_static_property_by_id(
        &mut self,
        id: InlineCacheId,
        kind: ClassConstantStaticPropertyCacheKind,
        resolved_class: &str,
        member: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> (
        Option<ClassConstantStaticPropertyCacheTarget>,
        InlineCacheObservation,
    ) {
        let Some(slot) = self.slot_by_id_mut(id) else {
            return (
                None,
                with_kind(
                    InlineCacheKind::ClassConstantStaticProperty,
                    InlineCacheObservation::miss(),
                ),
            );
        };
        if slot.kind() != InlineCacheKind::ClassConstantStaticProperty {
            return (
                None,
                with_kind(
                    InlineCacheKind::ClassConstantStaticProperty,
                    InlineCacheObservation::miss(),
                ),
            );
        }
        if slot.state == InlineCacheState::Disabled {
            return (
                None,
                with_kind(
                    InlineCacheKind::ClassConstantStaticProperty,
                    disabled_slot_observation(),
                ),
            );
        }
        if slot.state == InlineCacheState::Megamorphic {
            return (
                None,
                with_kind(
                    InlineCacheKind::ClassConstantStaticProperty,
                    megamorphic_slot_observation(),
                ),
            );
        }
        if slot.class_static_entries().is_empty() {
            let observation = record_slot_miss(slot);
            return (
                None,
                with_kind(InlineCacheKind::ClassConstantStaticProperty, observation),
            );
        }
        let Some(index) = slot.class_static_entries().iter().position(|entry| {
            class_static_guard_matches(
                ClassStaticGuard {
                    kind,
                    resolved_class,
                    member,
                    scope,
                },
                entry,
            )
        }) else {
            let observation = record_slot_guard_failure(slot);
            return (
                None,
                with_kind(InlineCacheKind::ClassConstantStaticProperty, observation),
            );
        };
        if slot.epoch != epoch {
            let observation = record_slot_invalidation(slot);
            return (
                None,
                with_kind(InlineCacheKind::ClassConstantStaticProperty, observation),
            );
        }
        let target = slot.class_static_entries()[index].target.clone();
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
        if let Some(id) = self.site_ids.get(&key).copied() {
            self.install_class_constant_static_property_by_id(
                id,
                kind,
                resolved_class,
                member,
                scope,
                epoch,
                target,
            );
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the dense install API carries the complete typed guard and target"
    )]
    pub fn install_class_constant_static_property_by_id(
        &mut self,
        id: InlineCacheId,
        kind: ClassConstantStaticPropertyCacheKind,
        resolved_class: &str,
        member: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
        target: ClassConstantStaticPropertyCacheTarget,
    ) {
        let Some(slot) = self.slot_by_id_mut(id) else {
            return;
        };
        if slot.kind() != InlineCacheKind::ClassConstantStaticProperty
            || matches!(
                slot.state,
                InlineCacheState::Disabled | InlineCacheState::Megamorphic
            )
        {
            return;
        }
        if !slot.class_static_entries().is_empty() && slot.epoch != epoch {
            clear_slot_targets(slot);
            slot.state = InlineCacheState::Cold;
        }
        let new_entry = ClassStaticCacheEntry {
            kind,
            resolved_class: resolved_class.to_owned(),
            member: member.to_owned(),
            scope: scope.map(str::to_owned),
            target,
        };
        if let Some(index) = slot.class_static_entries().iter().position(|entry| {
            class_static_guard_matches(
                ClassStaticGuard {
                    kind,
                    resolved_class,
                    member,
                    scope,
                },
                entry,
            )
        }) {
            slot.class_static_entries_mut()[index] = new_entry;
        } else {
            if slot.class_static_entries().len() >= POLYMORPHIC_INLINE_CACHE_LIMIT {
                mark_slot_megamorphic(slot);
                return;
            }
            slot.class_static_entries_mut().push(new_entry);
        }
        let entry_count = slot.class_static_entries().len();
        finish_polymorphic_install(slot, epoch, entry_count);
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
        let Some(id) = self.site_ids.get(&key).copied() else {
            return (
                None,
                include_path_observation(InlineCacheObservation::miss()),
            );
        };
        self.lookup_include_path_by_id(id, request, epoch)
    }

    pub fn lookup_include_path_by_id(
        &mut self,
        id: InlineCacheId,
        request: &IncludePathCacheKey,
        epoch: InvalidationEpoch,
    ) -> (Option<IncludePathCacheTarget>, InlineCacheObservation) {
        let include_path_observation =
            |observation: InlineCacheObservation| InlineCacheObservation {
                kind: Some(InlineCacheKind::IncludePath),
                ..observation
            };
        let Some(slot) = self.slot_by_id_mut(id) else {
            return (
                None,
                include_path_observation(InlineCacheObservation::miss()),
            );
        };
        if slot.kind() != InlineCacheKind::IncludePath {
            return (
                None,
                include_path_observation(InlineCacheObservation::miss()),
            );
        }
        if slot.state == InlineCacheState::Disabled {
            return (None, include_path_observation(disabled_slot_observation()));
        }
        if slot.state == InlineCacheState::Megamorphic {
            return (
                None,
                include_path_observation(megamorphic_slot_observation()),
            );
        }
        if slot.include_path_entries().is_empty() {
            return (None, include_path_observation(record_slot_miss(slot)));
        }
        let Some(index) = slot
            .include_path_entries()
            .iter()
            .position(|entry| &entry.key == request)
        else {
            return (
                None,
                include_path_observation(record_slot_guard_failure(slot)),
            );
        };
        if slot.epoch != epoch {
            return (
                None,
                include_path_observation(record_slot_invalidation(slot)),
            );
        }
        let target = slot.include_path_entries()[index].target.clone();
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
        if let Some(id) = self.site_ids.get(&key).copied() {
            return self.record_include_path_hit_by_id(id);
        }
        InlineCacheObservation {
            kind: Some(InlineCacheKind::IncludePath),
            ..InlineCacheObservation::hit()
        }
    }

    pub fn record_include_path_hit_by_id(&mut self, id: InlineCacheId) -> InlineCacheObservation {
        let observation = self
            .slot_by_id_mut(id)
            .filter(|slot| slot.kind() == InlineCacheKind::IncludePath)
            .map_or_else(InlineCacheObservation::hit, record_slot_hit);
        with_kind(InlineCacheKind::IncludePath, observation)
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
        if let Some(id) = self.site_ids.get(&key).copied() {
            return self.invalidate_include_path_by_id(id);
        }
        InlineCacheObservation {
            kind: Some(InlineCacheKind::IncludePath),
            ..InlineCacheObservation::invalidation()
        }
    }

    pub fn invalidate_include_path_by_id(&mut self, id: InlineCacheId) -> InlineCacheObservation {
        let observation = self
            .slot_by_id_mut(id)
            .filter(|slot| slot.kind() == InlineCacheKind::IncludePath)
            .map_or_else(
                InlineCacheObservation::invalidation,
                record_slot_invalidation,
            );
        with_kind(InlineCacheKind::IncludePath, observation)
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
        if let Some(id) = self.site_ids.get(&key).copied() {
            self.install_include_path_by_id(id, request, epoch, target);
        }
    }

    pub fn install_include_path_by_id(
        &mut self,
        id: InlineCacheId,
        request: IncludePathCacheKey,
        epoch: InvalidationEpoch,
        target: IncludePathCacheTarget,
    ) {
        let Some(slot) = self.slot_by_id_mut(id) else {
            return;
        };
        if slot.kind() != InlineCacheKind::IncludePath
            || matches!(
                slot.state,
                InlineCacheState::Disabled | InlineCacheState::Megamorphic
            )
        {
            return;
        }
        if !slot.include_path_entries().is_empty() && slot.epoch != epoch {
            clear_slot_targets(slot);
            slot.state = InlineCacheState::Cold;
        }
        let new_entry = IncludePathCacheEntry {
            key: request,
            target,
        };
        if let Some(index) = slot
            .include_path_entries()
            .iter()
            .position(|entry| entry.key == new_entry.key)
        {
            slot.include_path_entries_mut()[index] = new_entry;
        } else {
            if slot.include_path_entries().len() >= POLYMORPHIC_INLINE_CACHE_LIMIT {
                mark_slot_megamorphic(slot);
                return;
            }
            slot.include_path_entries_mut().push(new_entry);
        }
        let entry_count = slot.include_path_entries().len();
        finish_polymorphic_install(slot, epoch, entry_count);
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
        let Some(id) = self.site_ids.get(&key).copied() else {
            return (None, autoload_observation(InlineCacheObservation::miss()));
        };
        self.lookup_autoload_class_lookup_by_id(id, request, epochs)
    }

    pub fn lookup_autoload_class_lookup_by_id(
        &mut self,
        id: InlineCacheId,
        request: &AutoloadClassLookupCacheKey,
        epochs: AutoloadClassLookupEpochs,
    ) -> (
        Option<AutoloadClassLookupCacheTarget>,
        InlineCacheObservation,
    ) {
        let autoload_observation = |observation: InlineCacheObservation| InlineCacheObservation {
            kind: Some(InlineCacheKind::AutoloadClassLookup),
            ..observation
        };
        let Some(slot) = self.slot_by_id_mut(id) else {
            return (None, autoload_observation(InlineCacheObservation::miss()));
        };
        if slot.kind() != InlineCacheKind::AutoloadClassLookup {
            return (None, autoload_observation(InlineCacheObservation::miss()));
        }
        if slot.state == InlineCacheState::Disabled {
            return (None, autoload_observation(disabled_slot_observation()));
        }
        if slot.state == InlineCacheState::Megamorphic {
            return (None, autoload_observation(megamorphic_slot_observation()));
        }
        if slot.autoload_lookup_entries().is_empty() {
            return (None, autoload_observation(record_slot_miss(slot)));
        }
        let Some(index) = slot
            .autoload_lookup_entries()
            .iter()
            .position(|entry| &entry.key == request)
        else {
            return (None, autoload_observation(record_slot_guard_failure(slot)));
        };
        if slot.autoload_lookup_entries()[index].epochs != epochs {
            return (None, autoload_observation(record_slot_invalidation(slot)));
        }
        let target = slot.autoload_lookup_entries()[index].target.clone();
        (Some(target), autoload_observation(record_slot_hit(slot)))
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
        if let Some(id) = self.site_ids.get(&key).copied() {
            return self.invalidate_autoload_class_lookup_by_id(id);
        }
        InlineCacheObservation {
            kind: Some(InlineCacheKind::AutoloadClassLookup),
            ..InlineCacheObservation::invalidation()
        }
    }

    pub fn invalidate_autoload_class_lookup_by_id(
        &mut self,
        id: InlineCacheId,
    ) -> InlineCacheObservation {
        let observation = self
            .slot_by_id_mut(id)
            .filter(|slot| slot.kind() == InlineCacheKind::AutoloadClassLookup)
            .map_or_else(
                InlineCacheObservation::invalidation,
                record_slot_invalidation,
            );
        with_kind(InlineCacheKind::AutoloadClassLookup, observation)
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
        if let Some(id) = self.site_ids.get(&key).copied() {
            self.install_autoload_class_lookup_by_id(id, request, epochs, target);
        }
    }

    pub fn install_autoload_class_lookup_by_id(
        &mut self,
        id: InlineCacheId,
        request: AutoloadClassLookupCacheKey,
        epochs: AutoloadClassLookupEpochs,
        target: AutoloadClassLookupCacheTarget,
    ) {
        let Some(slot) = self.slot_by_id_mut(id) else {
            return;
        };
        if slot.kind() != InlineCacheKind::AutoloadClassLookup
            || matches!(
                slot.state,
                InlineCacheState::Disabled | InlineCacheState::Megamorphic
            )
        {
            return;
        }
        if slot
            .autoload_lookup_entries()
            .first()
            .is_some_and(|entry| entry.epochs != epochs)
        {
            clear_slot_targets(slot);
            slot.state = InlineCacheState::Cold;
        }
        let new_entry = AutoloadLookupCacheEntry {
            key: request,
            epochs,
            target,
        };
        if let Some(index) = slot
            .autoload_lookup_entries()
            .iter()
            .position(|entry| entry.key == new_entry.key)
        {
            slot.autoload_lookup_entries_mut()[index] = new_entry;
        } else {
            if slot.autoload_lookup_entries().len() >= POLYMORPHIC_INLINE_CACHE_LIMIT {
                mark_slot_megamorphic(slot);
                return;
            }
            slot.autoload_lookup_entries_mut().push(new_entry);
        }
        let entry_count = slot.autoload_lookup_entries().len();
        finish_polymorphic_install(
            slot,
            InvalidationEpoch::new(epochs.class_table_epoch),
            entry_count,
        );
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
#[path = "inline_cache/lifecycle_tests.rs"]
mod lifecycle_tests;

#[cfg(test)]
#[path = "inline_cache/tests.rs"]
mod tests;
