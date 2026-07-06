//! VM-owned deoptimization and live-state metadata.
//!
//! This module builds report-only metadata from verified dense bytecode. It is
//! intentionally independent of executable native code so future Cranelift,
//! baseline-native, or quickening tiers can consume the same resume contract.

use php_ir::instruction::{InstructionKind, IrCallArg, TerminatorKind};
use php_ir::{IrSpan, IrUnit};

use crate::aliasing::AliasState;
use crate::bytecode::{
    DENSE_BYTECODE_VERSION, DenseBlock, DenseBytecodeUnit, DenseFunction, DenseInstruction,
    DenseLowerError, DenseOpcode,
};

/// Stable VM-owned deoptimization reason codes.
///
/// Codes 1 through 7 intentionally match the existing Cranelift side-exit
/// reason codes so current counter reports remain compatible.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum VmDeoptReason {
    /// Runtime value type did not match the optimized specialization.
    TypeMismatch = 1,
    /// Checked arithmetic or conversion overflowed.
    Overflow = 2,
    /// Runtime value shape is outside the optimized subset.
    UnsupportedValue = 3,
    /// A generated guard failed.
    GuardFailed = 4,
    /// Runtime helper returned a non-OK status.
    HelperStatus = 5,
    /// PHP exception/error state is pending.
    ExceptionPending = 6,
    /// VM/native ABI hash or call boundary did not match.
    AbiMismatch = 7,
    /// Userland or builtin call frame state must be materialized first.
    CallFrameBoundary = 8,
    /// Reference/COW identity is not represented precisely enough.
    ReferenceCowIdentity = 9,
    /// Foreach iterator state must be materialized.
    ForeachIteratorState = 10,
    /// Pending finally/unwind state must be preserved.
    PendingFinally = 11,
    /// Generator or fiber suspension state must be preserved.
    GeneratorOrFiberState = 12,
    /// Output buffering/conversion state must stay interpreter-owned.
    OutputBufferState = 13,
    /// Control-flow shape is outside the metadata generator subset.
    UnsupportedControlFlow = 14,
}

impl VmDeoptReason {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TypeMismatch => "type_mismatch",
            Self::Overflow => "overflow",
            Self::UnsupportedValue => "unsupported_value",
            Self::GuardFailed => "guard_failed",
            Self::HelperStatus => "helper_status",
            Self::ExceptionPending => "exception_pending",
            Self::AbiMismatch => "abi_mismatch",
            Self::CallFrameBoundary => "call_frame_boundary",
            Self::ReferenceCowIdentity => "reference_cow_identity",
            Self::ForeachIteratorState => "foreach_iterator_state",
            Self::PendingFinally => "pending_finally",
            Self::GeneratorOrFiberState => "generator_or_fiber_state",
            Self::OutputBufferState => "output_buffer_state",
            Self::UnsupportedControlFlow => "unsupported_control_flow",
        }
    }

    /// Stable numeric report/ABI code.
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }
}

/// Stable state-family names used for snapshot rejection counters.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SnapshotStateFamily {
    InitializedLocals,
    Temporaries,
    ReferenceAliases,
    CowArrays,
    ObjectHandles,
    ForeachIterators,
    CallFrames,
    OutputBuffers,
    PendingDiagnostics,
    ExceptionFinally,
    IncludeStack,
    SourceTrace,
}

impl SnapshotStateFamily {
    /// Stable counter spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InitializedLocals => "initialized_locals",
            Self::Temporaries => "temporaries",
            Self::ReferenceAliases => "reference_aliases",
            Self::CowArrays => "cow_arrays",
            Self::ObjectHandles => "object_handles",
            Self::ForeachIterators => "foreach_iterators",
            Self::CallFrames => "call_frames",
            Self::OutputBuffers => "output_buffers",
            Self::PendingDiagnostics => "pending_diagnostics",
            Self::ExceptionFinally => "exception_finally",
            Self::IncludeStack => "include_stack",
            Self::SourceTrace => "source_trace",
        }
    }
}

/// One interpreter resume target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeoptResumePoint {
    /// Dense function index.
    pub function: u32,
    /// Dense block index.
    pub block: u32,
    /// Dense instruction index inside the function instruction array.
    pub instruction: u32,
}

/// Value class in a live-state snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveValueClass {
    /// VM register file slot.
    Register,
    /// PHP local variable slot.
    Local,
    /// Operand stack slot, reserved for future stack-based regions.
    OperandStack,
}

/// Whether a live value can carry reference/COW identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveIdentityMarker {
    /// The value is plain or identity is irrelevant for this snapshot.
    Plain,
    /// The value may be a reference cell or COW-backed container.
    MaybeReferenceOrCow,
}

/// One value slot recorded in a live-state snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LiveValueSlot {
    /// Value storage class.
    pub class: LiveValueClass,
    /// Zero-based index in that storage class.
    pub index: u32,
    /// Whether the value is definitely initialized at this point.
    pub initialized: Option<bool>,
    /// Reference/COW identity marker.
    pub identity: LiveIdentityMarker,
    /// Fine-grained alias class, aligned with the VM's reference-aliasing
    /// model (`AliasState`). Reported so a future tier can distinguish
    /// no-reference, local-only, escaped, global/superglobal,
    /// property/array-dim, and unknown aliasing. Current dense regions
    /// classify conservatively per region rather than per slot.
    pub alias_class: AliasState,
}

/// How a snapshot represents PHP control-flow state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlStateMarker {
    /// State is known absent for the generated dense region.
    None,
    /// State is present and explicitly represented in metadata.
    Represented,
    /// State exists but this metadata generator rejects the region.
    Rejected,
}

/// VM-owned live-state snapshot for optimized side exits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveStateSnapshot {
    /// Resume location and current bytecode location.
    pub resume: DeoptResumePoint,
    /// Source span for diagnostics and traces.
    pub span: IrSpan,
    /// Register file slots to materialize.
    pub registers: Vec<LiveValueSlot>,
    /// PHP local slots to materialize.
    pub locals: Vec<LiveValueSlot>,
    /// Operand stack slots, empty for the current register VM.
    pub operand_stack: Vec<LiveValueSlot>,
    /// Pending exception marker.
    pub pending_exception: ControlStateMarker,
    /// Pending finally/unwind marker.
    pub pending_finally: ControlStateMarker,
    /// Foreach iterator state marker.
    pub foreach_iterator: ControlStateMarker,
    /// Reference/COW state marker.
    pub reference_cow: ControlStateMarker,
    /// Output-buffer state marker.
    pub output_buffer: ControlStateMarker,
    /// Call frame identity marker.
    pub call_frame_identity: ControlStateMarker,
    /// Include stack marker.
    pub include_stack: ControlStateMarker,
    /// Pending diagnostics marker.
    pub pending_diagnostics: ControlStateMarker,
    /// Source/trace metadata marker.
    pub source_trace: ControlStateMarker,
}

impl LiveStateSnapshot {
    /// Materializes this optimized-exit snapshot into the generic VM resume
    /// contract. Current dense and quickened interpreter tiers already keep
    /// values in canonical frames, so materialization verifies exact state
    /// availability before resuming the generic helper path.
    pub fn materialize_for_generic_resume(
        &self,
    ) -> Result<MaterializedLiveState, SnapshotRejection> {
        if let Some(family) = self.rejected_state_family() {
            return Err(SnapshotRejection { family });
        }

        Ok(MaterializedLiveState {
            resume: self.resume,
            span: self.span,
            register_count: self.registers.len(),
            local_count: self.locals.len(),
            operand_stack_count: self.operand_stack.len(),
            represented: self.represented_state_families(),
        })
    }

    /// Returns the first state family that cannot be represented exactly.
    #[must_use]
    pub fn rejected_state_family(&self) -> Option<SnapshotStateFamily> {
        if self
            .locals
            .iter()
            .any(|slot| slot.initialized == Some(false))
        {
            return Some(SnapshotStateFamily::InitializedLocals);
        }
        if self
            .registers
            .iter()
            .any(|slot| slot.initialized == Some(false))
        {
            return Some(SnapshotStateFamily::Temporaries);
        }
        if self
            .registers
            .iter()
            .chain(self.locals.iter())
            .chain(self.operand_stack.iter())
            .any(|slot| slot.identity == LiveIdentityMarker::MaybeReferenceOrCow)
            && self.reference_cow == ControlStateMarker::Rejected
        {
            return Some(SnapshotStateFamily::ReferenceAliases);
        }
        if self.foreach_iterator == ControlStateMarker::Rejected {
            return Some(SnapshotStateFamily::ForeachIterators);
        }
        if self.pending_exception == ControlStateMarker::Rejected
            || self.pending_finally == ControlStateMarker::Rejected
        {
            return Some(SnapshotStateFamily::ExceptionFinally);
        }
        if self.output_buffer == ControlStateMarker::Rejected {
            return Some(SnapshotStateFamily::OutputBuffers);
        }
        if self.call_frame_identity == ControlStateMarker::Rejected {
            return Some(SnapshotStateFamily::CallFrames);
        }
        if self.include_stack == ControlStateMarker::Rejected {
            return Some(SnapshotStateFamily::IncludeStack);
        }
        if self.pending_diagnostics == ControlStateMarker::Rejected {
            return Some(SnapshotStateFamily::PendingDiagnostics);
        }
        if self.source_trace == ControlStateMarker::Rejected {
            return Some(SnapshotStateFamily::SourceTrace);
        }
        None
    }

    fn represented_state_families(&self) -> Vec<SnapshotStateFamily> {
        let mut families = vec![
            SnapshotStateFamily::InitializedLocals,
            SnapshotStateFamily::Temporaries,
            SnapshotStateFamily::CallFrames,
            SnapshotStateFamily::IncludeStack,
            SnapshotStateFamily::PendingDiagnostics,
            SnapshotStateFamily::SourceTrace,
        ];
        if self
            .registers
            .iter()
            .chain(self.locals.iter())
            .chain(self.operand_stack.iter())
            .any(|slot| slot.identity == LiveIdentityMarker::MaybeReferenceOrCow)
            || self.reference_cow == ControlStateMarker::Represented
        {
            families.push(SnapshotStateFamily::ReferenceAliases);
            families.push(SnapshotStateFamily::CowArrays);
            families.push(SnapshotStateFamily::ObjectHandles);
        }
        if self.foreach_iterator == ControlStateMarker::Represented {
            families.push(SnapshotStateFamily::ForeachIterators);
        }
        if self.output_buffer == ControlStateMarker::Represented {
            families.push(SnapshotStateFamily::OutputBuffers);
        }
        if self.pending_exception == ControlStateMarker::Represented
            || self.pending_finally == ControlStateMarker::Represented
        {
            families.push(SnapshotStateFamily::ExceptionFinally);
        }
        families.sort();
        families.dedup();
        families
    }

    /// Coarsest alias class across all live slots, aligned with the VM's
    /// reference-aliasing model. `NoReferencesObserved` means no live slot in
    /// the region carries reference/COW identity.
    #[must_use]
    pub fn reference_alias_summary(&self) -> AliasState {
        self.registers
            .iter()
            .chain(self.locals.iter())
            .chain(self.operand_stack.iter())
            .map(|slot| slot.alias_class)
            .max()
            .unwrap_or(AliasState::NoReferencesObserved)
    }

    /// Verifier rule for the alias-class metadata: a region that reports no
    /// reference/COW control state must not carry any reference-sensitive slot.
    /// Keeps the alias summary honest before a future tier consumes it.
    #[must_use]
    pub fn alias_metadata_consistent(&self) -> bool {
        let any_reference_sensitive = self
            .registers
            .iter()
            .chain(self.locals.iter())
            .chain(self.operand_stack.iter())
            .any(|slot| slot.alias_class.is_reference_sensitive());
        match self.reference_cow {
            ControlStateMarker::None => !any_reference_sensitive,
            ControlStateMarker::Represented | ControlStateMarker::Rejected => true,
        }
    }
}

/// Generic VM state restored from an optimized-exit snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializedLiveState {
    /// Interpreter resume location.
    pub resume: DeoptResumePoint,
    /// Source span used by diagnostics and trace metadata.
    pub span: IrSpan,
    /// Number of live registers represented.
    pub register_count: usize,
    /// Number of local slots represented.
    pub local_count: usize,
    /// Operand stack slots represented; zero for the current register VM.
    pub operand_stack_count: usize,
    /// Exact state families represented by the snapshot.
    pub represented: Vec<SnapshotStateFamily>,
}

/// Local snapshot rejection with one counted missing state family.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotRejection {
    /// Missing state family.
    pub family: SnapshotStateFamily,
}

/// One side-exit point from an optimized region back to the interpreter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeoptSideExitPoint {
    /// Stable reason.
    pub reason: VmDeoptReason,
    /// Interpreter resume location.
    pub resume: DeoptResumePoint,
    /// Live values and control markers at the exit.
    pub snapshot: LiveStateSnapshot,
}

/// One metadata region. FPE-16 uses dense basic blocks as conservative regions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeoptRegionMetadata {
    /// Stable region label.
    pub region_id: String,
    /// Dense function index.
    pub function: u32,
    /// Entry dense block index.
    pub entry_block: u32,
    /// Dense blocks covered by this region.
    pub blocks: Vec<u32>,
    /// Dense instruction indexes covered by this region.
    pub instructions: Vec<u32>,
    /// Side exits that can resume in the interpreter.
    pub side_exits: Vec<DeoptSideExitPoint>,
}

/// Report-only VM-owned deopt metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeoptMetadata {
    /// Metadata schema version.
    pub schema_version: u32,
    /// Dense bytecode version consumed by this metadata.
    pub dense_bytecode_version: u32,
    /// This foundation never enables native execution.
    pub native_execution: bool,
    /// Generated regions.
    pub regions: Vec<DeoptRegionMetadata>,
}

/// Stable guard identifier for guard/snapshot/resume metadata v2.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GuardId(u32);

impl GuardId {
    /// Creates a guard id.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw id.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Stable snapshot identifier for guard/snapshot/resume metadata v2.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SnapshotId(u32);

impl SnapshotId {
    /// Creates a snapshot id.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw id.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Stable exit identifier for guard/snapshot/resume metadata v2.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExitId(u32);

impl ExitId {
    /// Creates an exit id.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw id.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Guard family shared by adaptive interpreter and future native tiers.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum GuardKind {
    IntAdd,
    PropertyShape,
    PackedArray,
    BuiltinCall,
    QuickeningType,
    InlineCacheShape,
    RegionAssumption,
}

impl GuardKind {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IntAdd => "int_add",
            Self::PropertyShape => "property_shape",
            Self::PackedArray => "packed_array",
            Self::BuiltinCall => "builtin_call",
            Self::QuickeningType => "quickening_type",
            Self::InlineCacheShape => "inline_cache_shape",
            Self::RegionAssumption => "region_assumption",
        }
    }
}

/// Optimized tier or feature owning a guard.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum GuardedTier {
    Quickening,
    InlineCache,
    DenseBytecode,
    RegionIr,
    CopyPatch,
    Cranelift,
}

impl GuardedTier {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Quickening => "quickening",
            Self::InlineCache => "inline_cache",
            Self::DenseBytecode => "dense_bytecode",
            Self::RegionIr => "region_ir",
            Self::CopyPatch => "copy_patch",
            Self::Cranelift => "cranelift",
        }
    }
}

/// Guard-failure policy attached to a side exit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SideExitPolicy {
    GenericFallback,
    Dequicken,
    BlacklistForRequest,
    BlacklistPersistentlyCandidate,
    DisableFeature,
}

impl SideExitPolicy {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GenericFallback => "generic_fallback",
            Self::Dequicken => "dequicken",
            Self::BlacklistForRequest => "blacklist_for_request",
            Self::BlacklistPersistentlyCandidate => "blacklist_persistently_candidate",
            Self::DisableFeature => "disable_feature",
        }
    }
}

/// One live slot entry in a v2 snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotEntry {
    /// VM live slot.
    pub slot: LiveValueSlot,
    /// Stable type/value-class label when available.
    pub value_class: &'static str,
}

/// V2 snapshot with PHP state markers required for precise resume.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotRecord {
    /// Snapshot id.
    pub id: SnapshotId,
    /// Live VM slots.
    pub entries: Vec<SnapshotEntry>,
    /// Foreach state marker.
    pub foreach_state: ControlStateMarker,
    /// Exception/try/finally state marker.
    pub exception_or_finally_state: ControlStateMarker,
    /// Output-buffer state marker.
    pub output_buffer_state: ControlStateMarker,
    /// True when reference/COW state poisons optimized deopt.
    pub reference_cow_poisoned: bool,
}

/// Interpreter resume point used by guard/snapshot/resume metadata v2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResumePoint {
    /// Source function id.
    pub function: u32,
    /// Bytecode instruction offset.
    pub bytecode_offset: u32,
}

impl From<DeoptResumePoint> for ResumePoint {
    fn from(value: DeoptResumePoint) -> Self {
        Self {
            function: value.function,
            bytecode_offset: value.instruction,
        }
    }
}

/// One guard record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardRecord {
    /// Guard id.
    pub id: GuardId,
    /// Guard family.
    pub kind: GuardKind,
    /// Source function id.
    pub source_function: u32,
    /// Bytecode instruction offset.
    pub bytecode_offset: u32,
    /// Source span for diagnostics/traces.
    pub ir_span: Option<IrSpan>,
    /// Optimized tier or feature.
    pub tier: GuardedTier,
    /// Snapshot to restore on failure.
    pub snapshot: SnapshotId,
    /// Interpreter resume point.
    pub resume: ResumePoint,
    /// Stable exit reason.
    pub exit_reason: VmDeoptReason,
    /// Counter key for reports.
    pub counter_id: String,
    /// Blacklist/dequickening policy.
    pub policy: SideExitPolicy,
}

/// Shared side-exit label metadata. No machine label is stored here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedExit {
    /// Exit id.
    pub id: ExitId,
    /// Stable label for reports.
    pub label: String,
    /// Exit reason.
    pub reason: VmDeoptReason,
    /// Snapshot restore plan.
    pub snapshot: SnapshotId,
    /// Interpreter resume point.
    pub resume: ResumePoint,
}

/// Guard/snapshot/side-exit/resume metadata v2.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResumeTable {
    /// Versioned report schema.
    pub schema_version: u32,
    /// Guard records.
    pub guards: Vec<GuardRecord>,
    /// Snapshot records.
    pub snapshots: Vec<SnapshotRecord>,
    /// Shared exits.
    pub exits: Vec<SharedExit>,
}

impl Default for ResumeTable {
    fn default() -> Self {
        Self {
            schema_version: 2,
            guards: Vec::new(),
            snapshots: Vec::new(),
            exits: Vec::new(),
        }
    }
}

impl ResumeTable {
    /// Adds a snapshot and assigns the next stable id.
    pub fn add_snapshot(
        &mut self,
        entries: Vec<SnapshotEntry>,
        foreach_state: ControlStateMarker,
        exception_or_finally_state: ControlStateMarker,
        output_buffer_state: ControlStateMarker,
        reference_cow_poisoned: bool,
    ) -> SnapshotId {
        let id = SnapshotId::new(self.snapshots.len() as u32);
        self.snapshots.push(SnapshotRecord {
            id,
            entries,
            foreach_state,
            exception_or_finally_state,
            output_buffer_state,
            reference_cow_poisoned,
        });
        id
    }

    /// Adds a guard.
    pub fn add_guard(&mut self, mut guard: GuardRecord) -> GuardId {
        let id = GuardId::new(self.guards.len() as u32);
        guard.id = id;
        self.guards.push(guard);
        id
    }

    /// Adds a shared side exit.
    pub fn add_exit(
        &mut self,
        label: impl Into<String>,
        reason: VmDeoptReason,
        snapshot: SnapshotId,
        resume: ResumePoint,
    ) -> ExitId {
        let id = ExitId::new(self.exits.len() as u32);
        self.exits.push(SharedExit {
            id,
            label: label.into(),
            reason,
            snapshot,
            resume,
        });
        id
    }

    /// Materializes a snapshot by id for generic fallback resume.
    pub fn materialize_snapshot(
        &self,
        snapshot: SnapshotId,
    ) -> Result<MaterializedResumeRecord, SnapshotRejection> {
        let Some(record) = self.snapshots.get(snapshot.index()) else {
            return Err(SnapshotRejection {
                family: SnapshotStateFamily::SourceTrace,
            });
        };
        record.materialize_for_generic_resume()
    }

    /// Validates snapshot references and unsupported PHP state markers.
    pub fn validate(&self) -> Result<(), Vec<ResumeTableError>> {
        let mut errors = Vec::new();

        for guard in &self.guards {
            validate_snapshot_ref(self, guard.snapshot, "guard", guard.id.raw(), &mut errors);
            if let Some(snapshot) = self.snapshots.get(guard.snapshot.index()) {
                reject_unsupported_snapshot(snapshot, "guard", guard.id.raw(), &mut errors);
            }
        }
        for exit in &self.exits {
            validate_snapshot_ref(self, exit.snapshot, "exit", exit.id.raw(), &mut errors);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Compact versioned JSON report for tiering/JIT/performance stats.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut json = String::new();
        json.push_str("{\"schema_version\":");
        json.push_str(&self.schema_version.to_string());
        json.push_str(",\"guards\":[");
        for (index, guard) in self.guards.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str("{\"id\":");
            json.push_str(&guard.id.raw().to_string());
            json.push_str(",\"kind\":\"");
            json.push_str(guard.kind.as_str());
            json.push_str("\",\"tier\":\"");
            json.push_str(guard.tier.as_str());
            json.push_str("\",\"snapshot\":");
            json.push_str(&guard.snapshot.raw().to_string());
            json.push_str(",\"resume_offset\":");
            json.push_str(&guard.resume.bytecode_offset.to_string());
            json.push_str(",\"exit_reason\":\"");
            json.push_str(guard.exit_reason.as_str());
            json.push_str("\",\"policy\":\"");
            json.push_str(guard.policy.as_str());
            json.push_str("\"}");
        }
        json.push_str("],\"snapshots\":[");
        for (index, snapshot) in self.snapshots.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str("{\"id\":");
            json.push_str(&snapshot.id.raw().to_string());
            json.push_str(",\"entries\":");
            json.push_str(&snapshot.entries.len().to_string());
            json.push_str(",\"reference_cow_poisoned\":");
            json.push_str(if snapshot.reference_cow_poisoned {
                "true"
            } else {
                "false"
            });
            json.push('}');
        }
        json.push_str("],\"exits\":[");
        for (index, exit) in self.exits.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str("{\"id\":");
            json.push_str(&exit.id.raw().to_string());
            json.push_str(",\"reason\":\"");
            json.push_str(exit.reason.as_str());
            json.push_str("\",\"snapshot\":");
            json.push_str(&exit.snapshot.raw().to_string());
            json.push('}');
        }
        json.push_str("]}");
        json
    }
}

impl SnapshotRecord {
    /// Materializes this v2 snapshot record into a generic-resume summary.
    pub fn materialize_for_generic_resume(
        &self,
    ) -> Result<MaterializedResumeRecord, SnapshotRejection> {
        if self.reference_cow_poisoned {
            return Err(SnapshotRejection {
                family: SnapshotStateFamily::ReferenceAliases,
            });
        }
        if self.foreach_state == ControlStateMarker::Rejected {
            return Err(SnapshotRejection {
                family: SnapshotStateFamily::ForeachIterators,
            });
        }
        if self.exception_or_finally_state == ControlStateMarker::Rejected {
            return Err(SnapshotRejection {
                family: SnapshotStateFamily::ExceptionFinally,
            });
        }
        if self.output_buffer_state == ControlStateMarker::Rejected {
            return Err(SnapshotRejection {
                family: SnapshotStateFamily::OutputBuffers,
            });
        }
        Ok(MaterializedResumeRecord {
            snapshot: self.id,
            entry_count: self.entries.len(),
            foreach_state: self.foreach_state,
            exception_or_finally_state: self.exception_or_finally_state,
            output_buffer_state: self.output_buffer_state,
        })
    }
}

/// Materialized v2 resume-table snapshot summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializedResumeRecord {
    /// Snapshot id.
    pub snapshot: SnapshotId,
    /// Number of live entries restored.
    pub entry_count: usize,
    /// Foreach iterator materialization state.
    pub foreach_state: ControlStateMarker,
    /// Exception/finally materialization state.
    pub exception_or_finally_state: ControlStateMarker,
    /// Output-buffer materialization state.
    pub output_buffer_state: ControlStateMarker,
}

fn validate_snapshot_ref(
    table: &ResumeTable,
    snapshot: SnapshotId,
    owner: &'static str,
    owner_id: u32,
    errors: &mut Vec<ResumeTableError>,
) {
    if table.snapshots.get(snapshot.index()).is_none() {
        errors.push(ResumeTableError {
            code: "invalid_snapshot",
            detail: format!(
                "{} {} references missing snapshot s{}",
                owner,
                owner_id,
                snapshot.raw()
            ),
        });
    }
}

fn reject_unsupported_snapshot(
    snapshot: &SnapshotRecord,
    owner: &'static str,
    owner_id: u32,
    errors: &mut Vec<ResumeTableError>,
) {
    if snapshot.reference_cow_poisoned {
        errors.push(ResumeTableError {
            code: "reference_cow_poisoned",
            detail: format!(
                "{} {} snapshot carries reference/COW poison",
                owner, owner_id
            ),
        });
    }
    if snapshot.exception_or_finally_state == ControlStateMarker::Rejected {
        errors.push(ResumeTableError {
            code: "exception_or_finally_state_rejected",
            detail: format!("{} {} snapshot rejects try/finally state", owner, owner_id),
        });
    }
    if snapshot.foreach_state == ControlStateMarker::Rejected {
        errors.push(ResumeTableError {
            code: "foreach_state_rejected",
            detail: format!("{} {} snapshot rejects foreach state", owner, owner_id),
        });
    }
}

/// Resume-table validation error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResumeTableError {
    /// Machine-readable code.
    pub code: &'static str,
    /// Human-readable detail.
    pub detail: String,
}

impl DeoptMetadata {
    /// Generate metadata from rich IR by first rejecting unsupported VM state,
    /// then lowering to verified dense bytecode.
    pub fn generate_from_ir(unit: &IrUnit) -> Result<Self, Vec<DeoptMetadataError>> {
        let rejections = collect_ir_rejections(unit);
        if !rejections.is_empty() {
            return Err(rejections);
        }
        let dense = DenseBytecodeUnit::lower_from_ir(unit)
            .map_err(|error| vec![DeoptMetadataError::from_dense_lower_error(error)])?;
        Self::generate_from_dense(&dense)
    }

    /// Generate metadata from an already lowered dense bytecode unit.
    pub fn generate_from_dense(unit: &DenseBytecodeUnit) -> Result<Self, Vec<DeoptMetadataError>> {
        if let Err(errors) = unit.verify() {
            return Err(errors
                .into_iter()
                .map(|error| DeoptMetadataError {
                    reason: VmDeoptReason::UnsupportedControlFlow,
                    message: format!("dense bytecode verification failed: {}", error.message),
                })
                .collect());
        }

        let mut regions = Vec::new();
        for (function_index, function) in unit.functions.iter().enumerate() {
            for block in &function.blocks {
                regions.push(region_for_block(
                    unit,
                    function_index as u32,
                    function,
                    block,
                ));
            }
        }

        let metadata = Self {
            schema_version: 1,
            dense_bytecode_version: DENSE_BYTECODE_VERSION,
            native_execution: false,
            regions,
        };
        metadata.verify()?;
        Ok(metadata)
    }

    /// Verify metadata consistency against its own resume and live-state
    /// contract. Dense bytecode verification remains the source of bytecode
    /// structural truth.
    pub fn verify(&self) -> Result<(), Vec<DeoptMetadataError>> {
        let mut errors = Vec::new();
        if self.dense_bytecode_version != DENSE_BYTECODE_VERSION {
            errors.push(DeoptMetadataError {
                reason: VmDeoptReason::AbiMismatch,
                message: format!(
                    "metadata dense bytecode version {} does not match {}",
                    self.dense_bytecode_version, DENSE_BYTECODE_VERSION
                ),
            });
        }
        if self.native_execution {
            errors.push(DeoptMetadataError {
                reason: VmDeoptReason::UnsupportedControlFlow,
                message: "FPE-16 metadata must not enable native execution".to_string(),
            });
        }
        for region in &self.regions {
            if region.blocks.is_empty() || region.instructions.is_empty() {
                errors.push(DeoptMetadataError {
                    reason: VmDeoptReason::UnsupportedControlFlow,
                    message: format!("region {} is empty", region.region_id),
                });
            }
            for exit in &region.side_exits {
                if exit.resume.function != region.function {
                    errors.push(DeoptMetadataError {
                        reason: VmDeoptReason::UnsupportedControlFlow,
                        message: format!("region {} has cross-function resume", region.region_id),
                    });
                }
                if !region.blocks.contains(&exit.resume.block) {
                    errors.push(DeoptMetadataError {
                        reason: VmDeoptReason::UnsupportedControlFlow,
                        message: format!(
                            "region {} resume block {} is outside region blocks",
                            region.region_id, exit.resume.block
                        ),
                    });
                }
                if !region.instructions.contains(&exit.resume.instruction) {
                    errors.push(DeoptMetadataError {
                        reason: VmDeoptReason::UnsupportedControlFlow,
                        message: format!(
                            "region {} resume instruction {} is outside region instructions",
                            region.region_id, exit.resume.instruction
                        ),
                    });
                }
                verify_snapshot(region, &exit.snapshot, &mut errors);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Metadata generation/rejection error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeoptMetadataError {
    /// Stable reason.
    pub reason: VmDeoptReason,
    /// Human-readable detail.
    pub message: String,
}

impl DeoptMetadataError {
    fn from_dense_lower_error(error: DenseLowerError) -> Self {
        Self {
            reason: VmDeoptReason::UnsupportedControlFlow,
            message: format!("dense lowering rejected metadata region: {}", error.message),
        }
    }
}

fn region_for_block(
    unit: &DenseBytecodeUnit,
    function_index: u32,
    function: &DenseFunction,
    block: &DenseBlock,
) -> DeoptRegionMetadata {
    let first = block.first_instruction as usize;
    let end = first + block.instruction_len as usize;
    let instructions: Vec<u32> = (first as u32..end as u32).collect();
    let mut side_exits = Vec::new();
    for instruction_index in first..end {
        let instruction = &function.instructions[instruction_index];
        for reason in reasons_for_instruction(instruction) {
            let resume = DeoptResumePoint {
                function: function_index,
                block: block.id,
                instruction: instruction_index as u32,
            };
            side_exits.push(DeoptSideExitPoint {
                reason,
                resume,
                snapshot: snapshot_for_instruction(unit, function, instruction, resume, reason),
            });
        }
    }
    DeoptRegionMetadata {
        region_id: format!("f{function_index}:b{}", block.id),
        function: function_index,
        entry_block: block.id,
        blocks: vec![block.id],
        instructions,
        side_exits,
    }
}

fn snapshot_for_instruction(
    unit: &DenseBytecodeUnit,
    function: &DenseFunction,
    instruction: &DenseInstruction,
    resume: DeoptResumePoint,
    reason: VmDeoptReason,
) -> LiveStateSnapshot {
    let span = unit
        .spans
        .get(instruction.span.index())
        .copied()
        .unwrap_or_default();
    let value_identity = if matches!(
        reason,
        VmDeoptReason::ReferenceCowIdentity | VmDeoptReason::ForeachIteratorState
    ) {
        LiveIdentityMarker::MaybeReferenceOrCow
    } else {
        LiveIdentityMarker::Plain
    };
    // Conservative per-region alias class: a reference/COW deopt reason means
    // aliasing is not summarized precisely enough to trust, so report the
    // unknown class; every other supported region has no observed references.
    let region_alias_class = match reason {
        VmDeoptReason::ReferenceCowIdentity => AliasState::UnknownAliasing,
        _ => AliasState::NoReferencesObserved,
    };
    LiveStateSnapshot {
        resume,
        span,
        registers: (0..function.register_count)
            .map(|index| LiveValueSlot {
                class: LiveValueClass::Register,
                index,
                initialized: None,
                identity: value_identity,
                alias_class: region_alias_class,
            })
            .collect(),
        locals: (0..function.local_count)
            .map(|index| LiveValueSlot {
                class: LiveValueClass::Local,
                index,
                initialized: None,
                identity: value_identity,
                alias_class: region_alias_class,
            })
            .collect(),
        operand_stack: Vec::new(),
        pending_exception: marker_for(reason, VmDeoptReason::ExceptionPending),
        pending_finally: marker_for(reason, VmDeoptReason::PendingFinally),
        foreach_iterator: marker_for(reason, VmDeoptReason::ForeachIteratorState),
        reference_cow: marker_for(reason, VmDeoptReason::ReferenceCowIdentity),
        output_buffer: marker_for(reason, VmDeoptReason::OutputBufferState),
        call_frame_identity: marker_for(reason, VmDeoptReason::CallFrameBoundary),
        include_stack: ControlStateMarker::Represented,
        pending_diagnostics: ControlStateMarker::Represented,
        source_trace: ControlStateMarker::Represented,
    }
}

fn marker_for(actual: VmDeoptReason, target: VmDeoptReason) -> ControlStateMarker {
    if actual == target {
        ControlStateMarker::Represented
    } else {
        ControlStateMarker::None
    }
}

fn reasons_for_instruction(instruction: &DenseInstruction) -> Vec<VmDeoptReason> {
    match instruction.opcode {
        DenseOpcode::Nop
        | DenseOpcode::LoadConst
        | DenseOpcode::FetchConst
        | DenseOpcode::Move
        | DenseOpcode::StoreLocal
        | DenseOpcode::StoreLocalDiscard
        | DenseOpcode::UnsetLocal
        | DenseOpcode::IssetLocal
        | DenseOpcode::EmptyLocal
        | DenseOpcode::BindGlobal
        | DenseOpcode::Jump
        | DenseOpcode::Return
        | DenseOpcode::Exit
        | DenseOpcode::Discard => Vec::new(),
        DenseOpcode::LoadLocal
        | DenseOpcode::LoadLocalEcho
        | DenseOpcode::LoadLocalQuiet
        | DenseOpcode::LoadLocalLoadConst
        | DenseOpcode::LoadConstLoadConst
        | DenseOpcode::LoadConstArrayInsert => {
            vec![VmDeoptReason::UnsupportedValue]
        }
        DenseOpcode::BinaryAdd
        | DenseOpcode::BinarySub
        | DenseOpcode::BinaryMul
        | DenseOpcode::BinaryDiv
        | DenseOpcode::BinaryMod
        | DenseOpcode::BinaryConcat
        | DenseOpcode::BinaryPow
        | DenseOpcode::BinaryBitAnd
        | DenseOpcode::BinaryBitOr
        | DenseOpcode::BinaryBitXor
        | DenseOpcode::BinaryShiftLeft
        | DenseOpcode::BinaryShiftRight
        | DenseOpcode::BinaryConcatEcho
        | DenseOpcode::UnaryPlus
        | DenseOpcode::UnaryMinus
        | DenseOpcode::UnaryNot
        | DenseOpcode::UnaryBitNot
        | DenseOpcode::Cast
        | DenseOpcode::CompareEqual
        | DenseOpcode::CompareNotEqual
        | DenseOpcode::CompareIdentical
        | DenseOpcode::CompareNotIdentical
        | DenseOpcode::CompareLess
        | DenseOpcode::CompareLessEqual
        | DenseOpcode::CompareGreater
        | DenseOpcode::CompareGreaterEqual
        | DenseOpcode::CompareSpaceship => vec![
            VmDeoptReason::TypeMismatch,
            VmDeoptReason::Overflow,
            VmDeoptReason::HelperStatus,
        ],
        DenseOpcode::CallFunction
        | DenseOpcode::CallFunctionDiscard
        | DenseOpcode::NewObject
        | DenseOpcode::CallCallable
        | DenseOpcode::ResolveCallable
        | DenseOpcode::Pipe
        | DenseOpcode::AcquireCallable
        | DenseOpcode::MakeClosure
        | DenseOpcode::CallMethod
        | DenseOpcode::CallStaticMethod
        | DenseOpcode::Include
        | DenseOpcode::DeclareFunction
        | DenseOpcode::DeclareClass
        | DenseOpcode::FetchClassConstant
        | DenseOpcode::IssetProperty
        | DenseOpcode::EmptyProperty => vec![VmDeoptReason::CallFrameBoundary],
        DenseOpcode::IssetDim => vec![VmDeoptReason::HelperStatus],
        DenseOpcode::LoadConstEcho | DenseOpcode::Echo => {
            vec![VmDeoptReason::OutputBufferState]
        }
        DenseOpcode::NewArray
        | DenseOpcode::ArrayInsert
        | DenseOpcode::FetchDim
        | DenseOpcode::LoadConstFetchDim
        | DenseOpcode::AssignDim
        | DenseOpcode::AppendDim
        | DenseOpcode::BindReferenceDim
        | DenseOpcode::EmptyDim
        | DenseOpcode::UnsetDim
        | DenseOpcode::InitStaticLocal => vec![VmDeoptReason::ReferenceCowIdentity],
        DenseOpcode::FetchProperty | DenseOpcode::AssignProperty => vec![
            VmDeoptReason::GuardFailed,
            VmDeoptReason::HelperStatus,
            VmDeoptReason::ReferenceCowIdentity,
        ],
        DenseOpcode::InstanceOf => vec![VmDeoptReason::GuardFailed],
        DenseOpcode::IssetPropertyDim | DenseOpcode::EmptyPropertyDim => vec![
            VmDeoptReason::GuardFailed,
            VmDeoptReason::ReferenceCowIdentity,
        ],
        DenseOpcode::ForeachInit | DenseOpcode::ForeachNext | DenseOpcode::ForeachCleanup => {
            vec![VmDeoptReason::ForeachIteratorState]
        }
        DenseOpcode::JumpIfFalse | DenseOpcode::JumpIfTrue | DenseOpcode::JumpIf => {
            vec![VmDeoptReason::TypeMismatch, VmDeoptReason::GuardFailed]
        }
    }
}

fn collect_ir_rejections(unit: &IrUnit) -> Vec<DeoptMetadataError> {
    let mut errors = Vec::new();
    for (function_index, function) in unit.functions.iter().enumerate() {
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let Some(reason) = rejection_for_ir_instruction(&instruction.kind) {
                    errors.push(DeoptMetadataError {
                        reason,
                        message: format!(
                            "function {function_index} block {} instruction {} requires {}",
                            block.id.raw(),
                            instruction.id.raw(),
                            reason.as_str()
                        ),
                    });
                }
            }
            if let Some(terminator) = &block.terminator
                && let TerminatorKind::Return {
                    by_ref_local: Some(_),
                    ..
                } = terminator.kind
            {
                errors.push(DeoptMetadataError {
                    reason: VmDeoptReason::ReferenceCowIdentity,
                    message: format!(
                        "function {function_index} block {} by-reference return requires {}",
                        block.id.raw(),
                        VmDeoptReason::ReferenceCowIdentity.as_str()
                    ),
                });
            }
        }
    }
    errors
}

fn rejection_for_ir_instruction(kind: &InstructionKind) -> Option<VmDeoptReason> {
    match kind {
        InstructionKind::BindReference { .. }
        | InstructionKind::BindGlobal { .. }
        | InstructionKind::BindReferenceFromDim { .. }
        | InstructionKind::BindReferenceFromProperty { .. }
        | InstructionKind::BindReferenceFromCall { .. }
        | InstructionKind::BindReferenceFromMethodCall { .. }
        | InstructionKind::ForeachInitRef { .. }
        | InstructionKind::ForeachNextRef { .. } => Some(VmDeoptReason::ReferenceCowIdentity),
        InstructionKind::CallFunction { args, .. }
        | InstructionKind::CallMethod { args, .. }
        | InstructionKind::CallStaticMethod { args, .. }
        | InstructionKind::CallClosure { args, .. }
        | InstructionKind::CallCallable { args, .. }
        | InstructionKind::NewObject { args, .. }
        | InstructionKind::DynamicNewObject { args, .. }
            if args.iter().any(argument_needs_reference_metadata) =>
        {
            Some(VmDeoptReason::ReferenceCowIdentity)
        }
        InstructionKind::EnterTry { .. }
        | InstructionKind::LeaveTry
        | InstructionKind::EndFinally { .. } => Some(VmDeoptReason::PendingFinally),
        InstructionKind::Throw { .. } | InstructionKind::MakeException { .. } => {
            Some(VmDeoptReason::ExceptionPending)
        }
        InstructionKind::Yield { .. } | InstructionKind::YieldFrom { .. } => {
            Some(VmDeoptReason::GeneratorOrFiberState)
        }
        InstructionKind::Include { .. }
        | InstructionKind::Eval { .. }
        | InstructionKind::Unsupported { .. }
        | InstructionKind::RuntimeError { .. } => Some(VmDeoptReason::UnsupportedControlFlow),
        _ => None,
    }
}

fn argument_needs_reference_metadata(arg: &IrCallArg) -> bool {
    arg.by_ref_local.is_some()
        || arg.by_ref_dim.is_some()
        || arg.by_ref_property.is_some()
        || arg.by_ref_property_dim.is_some()
}

fn verify_snapshot(
    region: &DeoptRegionMetadata,
    snapshot: &LiveStateSnapshot,
    errors: &mut Vec<DeoptMetadataError>,
) {
    if snapshot.resume.function != region.function
        || !region.blocks.contains(&snapshot.resume.block)
        || !region.instructions.contains(&snapshot.resume.instruction)
    {
        errors.push(DeoptMetadataError {
            reason: VmDeoptReason::UnsupportedControlFlow,
            message: format!(
                "region {} snapshot resume {:?} is outside region",
                region.region_id, snapshot.resume
            ),
        });
    }
    for value in snapshot
        .registers
        .iter()
        .chain(snapshot.locals.iter())
        .chain(snapshot.operand_stack.iter())
    {
        if matches!(value.class, LiveValueClass::OperandStack) {
            errors.push(DeoptMetadataError {
                reason: VmDeoptReason::UnsupportedControlFlow,
                message: format!(
                    "region {} has operand-stack slot {} but the VM is register-based",
                    region.region_id, value.index
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata_from_source(source: &str) -> Result<DeoptMetadata, Vec<DeoptMetadataError>> {
        let frontend = php_semantics::analyze_source(source);
        let result = php_ir::lower_frontend_result(
            &frontend,
            php_ir::LoweringOptions {
                source_path: "fixtures/deopt/fpe16.php".to_string(),
                ..php_ir::LoweringOptions::default()
            },
        );
        result
            .verification
            .expect("IR should verify before deopt metadata");
        DeoptMetadata::generate_from_ir(&result.unit)
    }

    fn rejected_reasons(source: &str) -> Vec<VmDeoptReason> {
        metadata_from_source(source)
            .expect_err("source should be rejected")
            .into_iter()
            .map(|error| error.reason)
            .collect()
    }

    #[test]
    fn deopt_metadata_covers_straight_line_scalar_region() {
        let metadata = metadata_from_source("<?php $x = 1 + 2; echo $x;")
            .expect("straight-line scalar metadata");
        metadata.verify().expect("metadata verifies");
        assert!(!metadata.native_execution);
        assert!(
            metadata
                .regions
                .iter()
                .flat_map(|region| &region.side_exits)
                .any(|exit| exit.reason == VmDeoptReason::TypeMismatch)
        );
        assert!(
            metadata
                .regions
                .iter()
                .flat_map(|region| &region.side_exits)
                .all(|exit| exit.snapshot.operand_stack.is_empty())
        );
        let materialized = metadata
            .regions
            .iter()
            .flat_map(|region| &region.side_exits)
            .next()
            .expect("straight-line metadata has at least one side exit")
            .snapshot
            .materialize_for_generic_resume()
            .expect("straight-line snapshot materializes");
        assert!(materialized.register_count > 0);
        assert!(
            materialized
                .represented
                .contains(&SnapshotStateFamily::InitializedLocals)
        );
    }

    #[test]
    fn deopt_metadata_covers_branch_resume_points() {
        let metadata = metadata_from_source("<?php $x = 1; if ($x) { echo 1; } else { echo 2; }")
            .expect("branch metadata");
        assert!(metadata.regions.len() >= 3);
        assert!(
            metadata
                .regions
                .iter()
                .flat_map(|region| &region.side_exits)
                .any(|exit| exit.reason == VmDeoptReason::GuardFailed)
        );
    }

    #[test]
    fn deopt_metadata_covers_loop_regions() {
        let metadata =
            metadata_from_source("<?php $i = 0; while ($i < 3) { $i = $i + 1; } echo $i;")
                .expect("loop metadata");
        assert!(metadata.regions.len() >= 2);
        assert!(
            metadata
                .regions
                .iter()
                .flat_map(|region| &region.side_exits)
                .any(|exit| exit.resume.block != 0)
        );
    }

    #[test]
    fn deopt_metadata_represents_by_value_foreach_state() {
        let metadata = metadata_from_source(
            "<?php $items = [1, 2]; foreach ($items as $value) { echo $value; }",
        )
        .expect("foreach metadata");
        assert!(
            metadata
                .regions
                .iter()
                .flat_map(|region| &region.side_exits)
                .any(|exit| {
                    exit.reason == VmDeoptReason::ForeachIteratorState
                        && exit.snapshot.foreach_iterator == ControlStateMarker::Represented
                })
        );
    }

    #[test]
    fn deopt_metadata_rejects_try_finally_state() {
        let reasons = rejected_reasons("<?php try { echo 1; } finally { echo 2; }");
        assert!(reasons.contains(&VmDeoptReason::PendingFinally));
    }

    #[test]
    fn deopt_metadata_rejects_exception_paths() {
        let reasons = rejected_reasons("<?php throw new Exception('boom');");
        assert!(reasons.contains(&VmDeoptReason::ExceptionPending));
    }

    #[test]
    fn deopt_metadata_rejects_generator_or_fiber_state() {
        let reasons = rejected_reasons(
            "<?php function gen() { yield 1; } foreach (gen() as $v) { echo $v; }",
        );
        assert!(reasons.contains(&VmDeoptReason::GeneratorOrFiberState));
    }

    #[test]
    fn deopt_metadata_rejects_reference_cow_state() {
        let reasons = rejected_reasons("<?php $a = 1; $b =& $a; echo $b;");
        assert!(reasons.contains(&VmDeoptReason::ReferenceCowIdentity));
    }

    #[test]
    fn deopt_reason_codes_match_existing_cranelift_side_exit_prefix() {
        assert_eq!(VmDeoptReason::TypeMismatch.code(), 1);
        assert_eq!(VmDeoptReason::Overflow.code(), 2);
        assert_eq!(VmDeoptReason::UnsupportedValue.code(), 3);
        assert_eq!(VmDeoptReason::GuardFailed.code(), 4);
        assert_eq!(VmDeoptReason::HelperStatus.code(), 5);
        assert_eq!(VmDeoptReason::ExceptionPending.code(), 6);
        assert_eq!(VmDeoptReason::AbiMismatch.code(), 7);
    }

    #[test]
    fn resume_table_models_int_add_quickening_guard() {
        let mut table = ResumeTable::default();
        let snapshot = scalar_snapshot(&mut table);
        let guard = guard_record(
            GuardKind::IntAdd,
            GuardedTier::Quickening,
            snapshot,
            VmDeoptReason::TypeMismatch,
        );
        table.add_guard(guard);
        table.add_exit(
            "int_add_type_exit",
            VmDeoptReason::TypeMismatch,
            snapshot,
            ResumePoint {
                function: 0,
                bytecode_offset: 3,
            },
        );

        table.validate().expect("int-add guard table verifies");
        table
            .materialize_snapshot(snapshot)
            .expect("int-add guard snapshot materializes");
        let json = table.to_json();
        assert!(json.contains("\"schema_version\":2"));
        assert!(json.contains("\"kind\":\"int_add\""));
        assert!(json.contains("\"tier\":\"quickening\""));
    }

    #[test]
    fn live_state_snapshot_materializes_exact_runtime_families() {
        let snapshot = LiveStateSnapshot {
            resume: DeoptResumePoint {
                function: 0,
                block: 0,
                instruction: 4,
            },
            span: IrSpan::default(),
            registers: vec![LiveValueSlot {
                class: LiveValueClass::Register,
                index: 0,
                initialized: Some(true),
                identity: LiveIdentityMarker::MaybeReferenceOrCow,
                alias_class: AliasState::UnknownAliasing,
            }],
            locals: vec![LiveValueSlot {
                class: LiveValueClass::Local,
                index: 0,
                initialized: Some(true),
                identity: LiveIdentityMarker::MaybeReferenceOrCow,
                alias_class: AliasState::UnknownAliasing,
            }],
            operand_stack: Vec::new(),
            pending_exception: ControlStateMarker::Represented,
            pending_finally: ControlStateMarker::Represented,
            foreach_iterator: ControlStateMarker::Represented,
            reference_cow: ControlStateMarker::Represented,
            output_buffer: ControlStateMarker::Represented,
            call_frame_identity: ControlStateMarker::Represented,
            include_stack: ControlStateMarker::Represented,
            pending_diagnostics: ControlStateMarker::Represented,
            source_trace: ControlStateMarker::Represented,
        };

        let materialized = snapshot
            .materialize_for_generic_resume()
            .expect("all runtime families are represented");
        assert_eq!(materialized.register_count, 1);
        assert_eq!(materialized.local_count, 1);
        assert!(
            materialized
                .represented
                .contains(&SnapshotStateFamily::ReferenceAliases)
        );
        assert!(
            materialized
                .represented
                .contains(&SnapshotStateFamily::ForeachIterators)
        );
        assert!(
            materialized
                .represented
                .contains(&SnapshotStateFamily::OutputBuffers)
        );
        assert!(
            materialized
                .represented
                .contains(&SnapshotStateFamily::ExceptionFinally)
        );
    }

    #[test]
    fn alias_class_metadata_is_reported_and_verified() {
        // End-to-end: a straight-line scalar region observes no references, so
        // every generated side-exit snapshot summarizes as no-reference and
        // stays consistent with its reference/COW control state.
        let metadata = metadata_from_source("<?php $x = 1 + 2; echo $x;")
            .expect("straight-line scalar metadata");
        let snapshots: Vec<&LiveStateSnapshot> = metadata
            .regions
            .iter()
            .flat_map(|region| region.side_exits.iter())
            .map(|exit| &exit.snapshot)
            .collect();
        assert!(
            !snapshots.is_empty(),
            "region should carry side-exit snapshots"
        );
        for snapshot in &snapshots {
            assert!(
                snapshot.alias_metadata_consistent(),
                "generated snapshot alias metadata must be consistent: {snapshot:?}"
            );
            assert_eq!(
                snapshot.reference_alias_summary(),
                AliasState::NoReferencesObserved,
                "a scalar region observes no references"
            );
        }

        // The verifier rule catches a snapshot that reports no reference/COW
        // control state yet carries a reference-sensitive slot.
        let inconsistent = LiveStateSnapshot {
            resume: DeoptResumePoint {
                function: 0,
                block: 0,
                instruction: 0,
            },
            span: IrSpan::default(),
            registers: vec![LiveValueSlot {
                class: LiveValueClass::Register,
                index: 0,
                initialized: Some(true),
                identity: LiveIdentityMarker::Plain,
                alias_class: AliasState::EscapedReference,
            }],
            locals: Vec::new(),
            operand_stack: Vec::new(),
            pending_exception: ControlStateMarker::None,
            pending_finally: ControlStateMarker::None,
            foreach_iterator: ControlStateMarker::None,
            reference_cow: ControlStateMarker::None,
            output_buffer: ControlStateMarker::None,
            call_frame_identity: ControlStateMarker::None,
            include_stack: ControlStateMarker::None,
            pending_diagnostics: ControlStateMarker::None,
            source_trace: ControlStateMarker::None,
        };
        assert!(
            !inconsistent.alias_metadata_consistent(),
            "a reference-sensitive slot with no reference/COW state is inconsistent"
        );
        assert_eq!(
            inconsistent.reference_alias_summary(),
            AliasState::EscapedReference
        );
    }

    #[test]
    fn live_state_snapshot_rejects_only_missing_state_family() {
        let snapshot = LiveStateSnapshot {
            resume: DeoptResumePoint {
                function: 0,
                block: 0,
                instruction: 4,
            },
            span: IrSpan::default(),
            registers: vec![LiveValueSlot {
                class: LiveValueClass::Register,
                index: 0,
                initialized: Some(true),
                identity: LiveIdentityMarker::Plain,
                alias_class: AliasState::NoReferencesObserved,
            }],
            locals: vec![LiveValueSlot {
                class: LiveValueClass::Local,
                index: 0,
                initialized: Some(true),
                identity: LiveIdentityMarker::Plain,
                alias_class: AliasState::NoReferencesObserved,
            }],
            operand_stack: Vec::new(),
            pending_exception: ControlStateMarker::None,
            pending_finally: ControlStateMarker::None,
            foreach_iterator: ControlStateMarker::Rejected,
            reference_cow: ControlStateMarker::None,
            output_buffer: ControlStateMarker::None,
            call_frame_identity: ControlStateMarker::Represented,
            include_stack: ControlStateMarker::Represented,
            pending_diagnostics: ControlStateMarker::Represented,
            source_trace: ControlStateMarker::Represented,
        };

        let rejection = snapshot
            .materialize_for_generic_resume()
            .expect_err("foreach state is the exact missing family");
        assert_eq!(rejection.family, SnapshotStateFamily::ForeachIterators);
    }

    #[test]
    fn resume_table_models_property_shape_guard() {
        let mut table = ResumeTable::default();
        let snapshot = scalar_snapshot(&mut table);
        table.add_guard(guard_record(
            GuardKind::PropertyShape,
            GuardedTier::InlineCache,
            snapshot,
            VmDeoptReason::GuardFailed,
        ));

        table
            .validate()
            .expect("property-shape guard table verifies");
        assert!(table.to_json().contains("\"kind\":\"property_shape\""));
    }

    #[test]
    fn resume_table_models_packed_array_guard() {
        let mut table = ResumeTable::default();
        let snapshot = scalar_snapshot(&mut table);
        table.add_guard(guard_record(
            GuardKind::PackedArray,
            GuardedTier::DenseBytecode,
            snapshot,
            VmDeoptReason::UnsupportedValue,
        ));

        table.validate().expect("packed-array guard table verifies");
        assert!(table.to_json().contains("\"kind\":\"packed_array\""));
    }

    #[test]
    fn resume_table_models_builtin_call_guard() {
        let mut table = ResumeTable::default();
        let snapshot = scalar_snapshot(&mut table);
        table.add_guard(guard_record(
            GuardKind::BuiltinCall,
            GuardedTier::Cranelift,
            snapshot,
            VmDeoptReason::HelperStatus,
        ));

        table.validate().expect("builtin-call guard table verifies");
        assert!(table.to_json().contains("\"kind\":\"builtin_call\""));
    }

    #[test]
    fn resume_table_rejects_reference_cow_poison() {
        let mut table = ResumeTable::default();
        let snapshot = table.add_snapshot(
            vec![snapshot_entry(0)],
            ControlStateMarker::None,
            ControlStateMarker::None,
            ControlStateMarker::None,
            true,
        );
        table.add_guard(guard_record(
            GuardKind::RegionAssumption,
            GuardedTier::RegionIr,
            snapshot,
            VmDeoptReason::ReferenceCowIdentity,
        ));

        let errors = table
            .validate()
            .expect_err("reference/COW poisoned snapshot should fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "reference_cow_poisoned")
        );
    }

    #[test]
    fn resume_table_rejects_try_finally_or_generator_state() {
        let mut table = ResumeTable::default();
        let try_finally = table.add_snapshot(
            vec![snapshot_entry(0)],
            ControlStateMarker::None,
            ControlStateMarker::Rejected,
            ControlStateMarker::None,
            false,
        );
        table.add_guard(guard_record(
            GuardKind::RegionAssumption,
            GuardedTier::RegionIr,
            try_finally,
            VmDeoptReason::PendingFinally,
        ));
        let generator = table.add_snapshot(
            vec![snapshot_entry(1)],
            ControlStateMarker::Rejected,
            ControlStateMarker::None,
            ControlStateMarker::None,
            false,
        );
        table.add_guard(guard_record(
            GuardKind::RegionAssumption,
            GuardedTier::RegionIr,
            generator,
            VmDeoptReason::GeneratorOrFiberState,
        ));

        let errors = table
            .validate()
            .expect_err("unsupported control state should fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "exception_or_finally_state_rejected")
        );
        assert!(
            errors
                .iter()
                .any(|error| error.code == "foreach_state_rejected")
        );
    }

    fn scalar_snapshot(table: &mut ResumeTable) -> SnapshotId {
        table.add_snapshot(
            vec![snapshot_entry(0), snapshot_entry(1)],
            ControlStateMarker::None,
            ControlStateMarker::None,
            ControlStateMarker::None,
            false,
        )
    }

    fn snapshot_entry(index: u32) -> SnapshotEntry {
        SnapshotEntry {
            slot: LiveValueSlot {
                class: LiveValueClass::Register,
                index,
                initialized: Some(true),
                identity: LiveIdentityMarker::Plain,
                alias_class: AliasState::NoReferencesObserved,
            },
            value_class: "i64",
        }
    }

    fn guard_record(
        kind: GuardKind,
        tier: GuardedTier,
        snapshot: SnapshotId,
        exit_reason: VmDeoptReason,
    ) -> GuardRecord {
        GuardRecord {
            id: GuardId::new(u32::MAX),
            kind,
            source_function: 0,
            bytecode_offset: 3,
            ir_span: Some(IrSpan::default()),
            tier,
            snapshot,
            resume: ResumePoint {
                function: 0,
                bytecode_offset: 3,
            },
            exit_reason,
            counter_id: format!("{}.{}", tier.as_str(), kind.as_str()),
            policy: SideExitPolicy::GenericFallback,
        }
    }
}
