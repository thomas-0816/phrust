//! Advisory persistent feedback metadata for future cross-request acceleration.
//!
//! This module owns metadata shape and invalidation only. It never stores
//! request-local VM values, object handles, arrays, resources, or non-interned
//! request strings, and the VM does not currently use accepted entries to alter
//! execution.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::quickening::{
    QuickeningSiteKey, QuickeningSiteSnapshot, QuickeningSpecialization, QuickeningState,
};

/// Stable line-format header for advisory persistent feedback files.
pub const PERSISTENT_FEEDBACK_FORMAT_VERSION: &str = "phrust-persistent-feedback-v1";

/// Upper bound on a persisted callsite's argument count. Real PHP signatures
/// never approach this; the cap only stops a corrupt/tampered sidecar from
/// forcing a large allocation when the seeder materializes the by-ref shape.
pub const MAX_PERSISTED_CALL_ARITY: u32 = 4096;

/// JSON/report schema version for persistent feedback stats.
///
/// v2 splits the collapsed `rejected_stale` counter into explicit
/// epoch/architecture/config mismatch reasons and adds `entries_written`
/// for the engine-owned writer path.
pub const PERSISTENT_FEEDBACK_STATS_SCHEMA_VERSION: u32 = 3;

/// Invalidation epochs that must match before feedback can be advisory input.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PersistentFeedbackEpochs {
    pub class_table: u64,
    pub function_table: u64,
    pub autoload: u64,
    pub include_path: u64,
}

/// How a load validates entry epochs against the context.
///
/// Entries record the invalidation epochs of the run that *observed* them.
/// A live in-process consumer knows its current epochs and can require an
/// exact match. A cold-start load (the CLI reading a sidecar before any code
/// has executed) cannot know the epochs this run will reach — for a matching
/// source/config/IR fingerprint the declaration sequence replays
/// deterministically, so the recorded epochs are the expectation, and every
/// consumer re-validates against live state at seed or lookup time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentFeedbackEpochValidation {
    /// Entries must match the context's epochs exactly.
    Exact,
    /// Accept recorded epochs; consumers re-validate against live state.
    DeferToConsumption,
}

/// Current-source context used to validate persisted feedback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistentFeedbackContext {
    pub source_fingerprint: String,
    pub engine_version: String,
    pub php_target_version: String,
    pub compile_options: String,
    pub ir_fingerprint: String,
    pub epochs: PersistentFeedbackEpochs,
    pub epoch_validation: PersistentFeedbackEpochValidation,
    pub target_arch_config: String,
}

impl PersistentFeedbackContext {
    #[must_use]
    pub fn new(
        source_fingerprint: impl Into<String>,
        engine_version: impl Into<String>,
        php_target_version: impl Into<String>,
        compile_options: impl Into<String>,
        ir_fingerprint: impl Into<String>,
        epochs: PersistentFeedbackEpochs,
        target_arch_config: impl Into<String>,
    ) -> Self {
        Self {
            source_fingerprint: source_fingerprint.into(),
            engine_version: engine_version.into(),
            php_target_version: php_target_version.into(),
            compile_options: compile_options.into(),
            ir_fingerprint: ir_fingerprint.into(),
            epochs,
            epoch_validation: PersistentFeedbackEpochValidation::Exact,
            target_arch_config: target_arch_config.into(),
        }
    }

    /// Returns the context with the given epochs, e.g. the final epochs of
    /// the run whose observations are being written.
    #[must_use]
    pub fn with_epochs(mut self, epochs: PersistentFeedbackEpochs) -> Self {
        self.epochs = epochs;
        self
    }

    /// Returns the context with the given epoch-validation policy.
    #[must_use]
    pub fn with_epoch_validation(mut self, policy: PersistentFeedbackEpochValidation) -> Self {
        self.epoch_validation = policy;
        self
    }

    #[must_use]
    pub fn validate_bytes(&self, bytes: &[u8]) -> PersistentFeedbackLoadReport {
        let mut stats = PersistentFeedbackStats {
            files_considered: 1,
            metadata_bytes: bytes.len() as u64,
            ..PersistentFeedbackStats::default()
        };
        let text = match std::str::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => {
                stats.rejected_corrupt = 1;
                stats.fallback_to_baseline = true;
                return PersistentFeedbackLoadReport::new(
                    PersistentFeedbackStore::default(),
                    stats,
                );
            }
        };
        let mut lines = text.lines();
        match lines.next().map(str::trim) {
            Some(PERSISTENT_FEEDBACK_FORMAT_VERSION) => {
                stats.files_loaded = 1;
            }
            _ => {
                stats.rejected_corrupt = 1;
                stats.fallback_to_baseline = true;
                return PersistentFeedbackLoadReport::new(
                    PersistentFeedbackStore::default(),
                    stats,
                );
            }
        }

        let mut store = PersistentFeedbackStore::default();
        for line in lines {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            stats.entries_seen = stats.entries_seen.saturating_add(1);
            match parse_entry_line(line) {
                Ok(entry) => match self.validate_entry(entry) {
                    Ok(entry) => {
                        stats.entries_accepted = stats.entries_accepted.saturating_add(1);
                        store.entries.push(entry);
                    }
                    Err(PersistentFeedbackRejectReason::Stale) => {
                        stats.rejected_stale = stats.rejected_stale.saturating_add(1);
                        stats.fallback_to_baseline = true;
                    }
                    Err(PersistentFeedbackRejectReason::EpochMismatch) => {
                        stats.rejected_epoch_mismatch =
                            stats.rejected_epoch_mismatch.saturating_add(1);
                        stats.fallback_to_baseline = true;
                    }
                    Err(PersistentFeedbackRejectReason::ArchitectureMismatch) => {
                        stats.rejected_architecture_mismatch =
                            stats.rejected_architecture_mismatch.saturating_add(1);
                        stats.fallback_to_baseline = true;
                    }
                    Err(PersistentFeedbackRejectReason::ConfigMismatch) => {
                        stats.rejected_config_mismatch =
                            stats.rejected_config_mismatch.saturating_add(1);
                        stats.fallback_to_baseline = true;
                    }
                    Err(PersistentFeedbackRejectReason::UserlandState) => {
                        stats.rejected_userland_state =
                            stats.rejected_userland_state.saturating_add(1);
                        stats.fallback_to_baseline = true;
                    }
                    Err(PersistentFeedbackRejectReason::Corrupt) => {
                        stats.rejected_corrupt = stats.rejected_corrupt.saturating_add(1);
                        stats.fallback_to_baseline = true;
                    }
                },
                Err(PersistentFeedbackRejectReason::UserlandState) => {
                    stats.rejected_userland_state = stats.rejected_userland_state.saturating_add(1);
                    stats.fallback_to_baseline = true;
                }
                Err(_) => {
                    stats.rejected_corrupt = stats.rejected_corrupt.saturating_add(1);
                    stats.fallback_to_baseline = true;
                }
            }
        }

        PersistentFeedbackLoadReport::new(store, stats)
    }

    /// Renders exported quickening sites into the v1 line format this
    /// context will accept back on the next load.
    #[must_use]
    pub fn render_sites(&self, sites: &[QuickeningSiteSnapshot]) -> String {
        self.render_sites_counted(sites).0
    }

    /// Engine-owned writer: renders exported quickening sites into the v1 line
    /// format and reports how many validator-accepted entries were emitted, so
    /// callers can record `entries_written` without re-parsing the output. The
    /// emitted entries carry this context's epochs, so a writer fed real
    /// invalidation epochs persists non-zero epochs.
    #[must_use]
    pub fn render_sites_counted(&self, sites: &[QuickeningSiteSnapshot]) -> (String, u64) {
        let mut written = 0u64;
        let mut text = String::with_capacity(64 + sites.len() * 256);
        text.push_str(PERSISTENT_FEEDBACK_FORMAT_VERSION);
        text.push('\n');
        for snapshot in sites {
            let (function, instruction, site_fields) = match snapshot.site {
                QuickeningSiteKey::Ir {
                    function,
                    block,
                    instruction,
                } => (function, instruction, format!("site=ir block={block}")),
                QuickeningSiteKey::Dense {
                    unit,
                    function,
                    instruction,
                } => (
                    function,
                    instruction,
                    format!("site=dense dense_unit={unit}"),
                ),
            };
            let (quickening_state, callsite_state, blacklisted) = match snapshot.state {
                QuickeningState::Specialized => ("specialized", "monomorphic", false),
                QuickeningState::Blacklisted => ("blacklisted", "blacklisted", true),
                QuickeningState::Uninitialized
                | QuickeningState::Observing
                | QuickeningState::Dequickened => continue,
            };
            let (specialization, scalars, array_fields) = match snapshot.specialization {
                Some(QuickeningSpecialization::AddIntInt) => ("add_int_int", "int,int", ""),
                Some(QuickeningSpecialization::SubIntInt) => ("sub_int_int", "int,int", ""),
                Some(QuickeningSpecialization::MulIntInt) => ("mul_int_int", "int,int", ""),
                Some(QuickeningSpecialization::ConcatStringString) => {
                    ("concat_string_string", "string,string", "")
                }
                Some(QuickeningSpecialization::PackedArrayIntKey) => (
                    "packed_array_int_key",
                    "array",
                    " array_layout=packed array_key=int",
                ),
                Some(QuickeningSpecialization::BoolBranchCondition) => {
                    ("bool_branch_condition", "bool", "")
                }
                None => {
                    if snapshot.state == QuickeningState::Specialized {
                        continue;
                    }
                    ("none", "unknown", "")
                }
            };
            let _ = writeln!(
                text,
                "entry source={} engine={} php={} compile={} function={function} ir={} \
                 instruction={instruction} class_epoch={} function_epoch={} autoload_epoch={} \
                 include_epoch={} target={} state={callsite_state} scalars={scalars}\
                 {array_fields} guard_failures={} blacklisted={blacklisted} {site_fields} \
                 quickening_state={quickening_state} specialization={specialization}",
                self.source_fingerprint,
                self.engine_version,
                self.php_target_version,
                self.compile_options,
                self.ir_fingerprint,
                self.epochs.class_table,
                self.epochs.function_table,
                self.epochs.autoload,
                self.epochs.include_path,
                self.target_arch_config,
                snapshot.guard_failures,
            );
            written = written.saturating_add(1);
        }
        (text, written)
    }

    /// Renders quickening sites plus monomorphic entry-unit function-call IC
    /// sites, returning the rendered text and how many entries it contains.
    #[must_use]
    pub fn render_feedback_counted(
        &self,
        sites: &[QuickeningSiteSnapshot],
        callsites: &[crate::inline_cache::FunctionCallSiteSnapshot],
    ) -> (String, u64) {
        let (mut text, mut written) = self.render_sites_counted(sites);
        for site in callsites {
            let _ = writeln!(
                text,
                "entry source={} engine={} php={} compile={} function={} ir={} \
                 instruction={} class_epoch={} function_epoch={} autoload_epoch={} \
                 include_epoch={} target={} state=monomorphic site=ic_function_call \
                 ic_block={} call_name={} call_arity={} call_site_epoch={} \
                 call_target_function={}",
                self.source_fingerprint,
                self.engine_version,
                self.php_target_version,
                self.compile_options,
                site.function,
                self.ir_fingerprint,
                site.instruction,
                self.epochs.class_table,
                self.epochs.function_table,
                self.epochs.autoload,
                self.epochs.include_path,
                self.target_arch_config,
                site.block,
                site.lowered_name,
                site.arity,
                site.epoch,
                site.target_function,
            );
            written = written.saturating_add(1);
        }
        (text, written)
    }

    fn validate_entry(
        &self,
        entry: PersistentFeedbackEntry,
    ) -> Result<PersistentFeedbackEntry, PersistentFeedbackRejectReason> {
        if entry.contains_userland_state {
            return Err(PersistentFeedbackRejectReason::UserlandState);
        }
        let key = &entry.key;
        // Distinguish rejection classes so callers can tell an out-of-date
        // deployment (config/arch/epoch) from a genuinely stale source. Order
        // matters only for attribution; any single mismatch rejects the entry.
        if key.compile_options != self.compile_options {
            return Err(PersistentFeedbackRejectReason::ConfigMismatch);
        }
        if key.target_arch_config != self.target_arch_config {
            return Err(PersistentFeedbackRejectReason::ArchitectureMismatch);
        }
        if self.epoch_validation == PersistentFeedbackEpochValidation::Exact
            && key.epochs != self.epochs
        {
            return Err(PersistentFeedbackRejectReason::EpochMismatch);
        }
        if key.source_fingerprint != self.source_fingerprint
            || key.engine_version != self.engine_version
            || key.php_target_version != self.php_target_version
            || key.ir_fingerprint != self.ir_fingerprint
        {
            return Err(PersistentFeedbackRejectReason::Stale);
        }
        Ok(entry)
    }
}

/// Stable key dimensions for one feedback slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistentFeedbackKey {
    pub source_fingerprint: String,
    pub engine_version: String,
    pub php_target_version: String,
    pub compile_options: String,
    pub function_id: u32,
    pub ir_fingerprint: String,
    pub instruction_id: u32,
    pub epochs: PersistentFeedbackEpochs,
    pub target_arch_config: String,
}

/// Advisory persistent feedback entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistentFeedbackEntry {
    pub key: PersistentFeedbackKey,
    pub payload: PersistentFeedbackPayload,
    contains_userland_state: bool,
}

/// Accepted metadata-only feedback entries.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PersistentFeedbackStore {
    entries: Vec<PersistentFeedbackEntry>,
}

impl PersistentFeedbackStore {
    #[must_use]
    pub fn entries(&self) -> &[PersistentFeedbackEntry] {
        &self.entries
    }
}

/// Result of loading one advisory feedback source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistentFeedbackLoadReport {
    pub store: PersistentFeedbackStore,
    pub stats: PersistentFeedbackStats,
}

impl PersistentFeedbackLoadReport {
    #[must_use]
    pub const fn new(store: PersistentFeedbackStore, stats: PersistentFeedbackStats) -> Self {
        Self { store, stats }
    }
}

/// Feedback load/validation counters. These are reported outside PHP stdout.
///
/// `advisory_only` reports the consumption *policy* of the run that produced
/// the stats: `true` means accepted entries could not seed adaptive VM state.
/// `consume_mode` names the resolved mode (`off` or `quickening`). The
/// validator itself never consumes, so both default to the advisory reading;
/// the embedding CLI/server stamps the real policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistentFeedbackStats {
    pub schema_version: u32,
    pub advisory_only: bool,
    pub consume_mode: &'static str,
    pub default_enabled: bool,
    pub files_considered: u64,
    pub files_loaded: u64,
    pub entries_seen: u64,
    pub entries_accepted: u64,
    pub entries_written: u64,
    pub rejected_stale: u64,
    pub rejected_epoch_mismatch: u64,
    pub rejected_architecture_mismatch: u64,
    pub rejected_config_mismatch: u64,
    pub rejected_corrupt: u64,
    pub rejected_userland_state: u64,
    pub fallback_to_baseline: bool,
    pub metadata_bytes: u64,
}

impl Default for PersistentFeedbackStats {
    fn default() -> Self {
        Self {
            schema_version: PERSISTENT_FEEDBACK_STATS_SCHEMA_VERSION,
            advisory_only: true,
            consume_mode: "off",
            default_enabled: false,
            files_considered: 0,
            files_loaded: 0,
            entries_seen: 0,
            entries_accepted: 0,
            entries_written: 0,
            rejected_stale: 0,
            rejected_epoch_mismatch: 0,
            rejected_architecture_mismatch: 0,
            rejected_config_mismatch: 0,
            rejected_corrupt: 0,
            rejected_userland_state: 0,
            fallback_to_baseline: false,
            metadata_bytes: 0,
        }
    }
}

impl PersistentFeedbackStats {
    #[must_use]
    pub fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\n",
                "  \"schema_version\": {},\n",
                "  \"advisory_only\": {},\n",
                "  \"consume_mode\": \"{}\",\n",
                "  \"default_enabled\": {},\n",
                "  \"files_considered\": {},\n",
                "  \"files_loaded\": {},\n",
                "  \"entries_seen\": {},\n",
                "  \"entries_accepted\": {},\n",
                "  \"entries_written\": {},\n",
                "  \"rejected_stale\": {},\n",
                "  \"rejected_epoch_mismatch\": {},\n",
                "  \"rejected_architecture_mismatch\": {},\n",
                "  \"rejected_config_mismatch\": {},\n",
                "  \"rejected_corrupt\": {},\n",
                "  \"rejected_userland_state\": {},\n",
                "  \"fallback_to_baseline\": {},\n",
                "  \"metadata_bytes\": {}\n",
                "}}\n"
            ),
            self.schema_version,
            self.advisory_only,
            self.consume_mode,
            self.default_enabled,
            self.files_considered,
            self.files_loaded,
            self.entries_seen,
            self.entries_accepted,
            self.entries_written,
            self.rejected_stale,
            self.rejected_epoch_mismatch,
            self.rejected_architecture_mismatch,
            self.rejected_config_mismatch,
            self.rejected_corrupt,
            self.rejected_userland_state,
            self.fallback_to_baseline,
            self.metadata_bytes
        )
    }
}

/// Monomorphic/polymorphic callsite state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PersistentCallsiteState {
    #[default]
    Cold,
    Monomorphic,
    Polymorphic,
    Megamorphic,
    Blacklisted,
}

/// Scalar metadata observed at a feedback site.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentScalarKind {
    Null,
    Bool,
    Int,
    Float,
    String,
    NumericString,
    Array,
    Object,
    Resource,
    Unknown,
}

/// Array layout metadata only, never array contents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentArrayLayout {
    Empty,
    Packed,
    Mixed,
    Unknown,
}

/// Observed array key shape metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentArrayKeyShape {
    IntOnly,
    StringOnly,
    IntString,
    Unknown,
}

/// Object shape metadata only, never object handles or property values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PersistentObjectShapeObservation {
    pub class_id: u32,
    pub layout_epoch: u64,
    pub property_slot: Option<u32>,
}

/// Branch bias metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentBranchBias {
    Unknown,
    MostlyTrue,
    MostlyFalse,
    Balanced,
}

/// Include/autoload target stability summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistentIncludeAutoloadStability {
    pub stable: bool,
    pub target_fingerprint: Option<String>,
}

/// Guard failure and blacklist summary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PersistentGuardFailureSummary {
    pub failures: u64,
    pub blacklisted: bool,
}

/// Metadata-only payload for one feedback slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistentFeedbackPayload {
    pub callsite_state: PersistentCallsiteState,
    pub scalar_kinds: Vec<PersistentScalarKind>,
    pub array_layout: Option<PersistentArrayLayout>,
    pub array_key_shape: Option<PersistentArrayKeyShape>,
    pub object_shape: Option<PersistentObjectShapeObservation>,
    pub branch_bias: Option<PersistentBranchBias>,
    pub include_autoload: Option<PersistentIncludeAutoloadStability>,
    pub guard_failures: PersistentGuardFailureSummary,
    /// Adaptive quickening site snapshot, when the entry carries one.
    pub quickening: Option<QuickeningSiteSnapshot>,
    /// Monomorphic entry-unit function-call IC site, when the entry carries
    /// one (see `FunctionCallSiteSnapshot` for the persistable subset).
    pub function_callsite: Option<crate::inline_cache::FunctionCallSiteSnapshot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PersistentFeedbackRejectReason {
    /// Source/engine/PHP-target/IR identity no longer matches.
    Stale,
    /// Class/function/autoload/include invalidation epoch mismatch.
    EpochMismatch,
    /// Target architecture/config label mismatch.
    ArchitectureMismatch,
    /// Compile-options (opt level, exec format, quickening, IC, cache, JIT,
    /// tiering) mismatch.
    ConfigMismatch,
    Corrupt,
    UserlandState,
}

fn parse_entry_line(line: &str) -> Result<PersistentFeedbackEntry, PersistentFeedbackRejectReason> {
    let mut parts = line.split_whitespace();
    if parts.next() != Some("entry") {
        return Err(PersistentFeedbackRejectReason::Corrupt);
    }
    let mut fields = BTreeMap::new();
    for part in parts {
        let Some((name, value)) = part.split_once('=') else {
            return Err(PersistentFeedbackRejectReason::Corrupt);
        };
        if name.is_empty() {
            return Err(PersistentFeedbackRejectReason::Corrupt);
        }
        fields.insert(name, value);
    }

    let contains_userland_state = contains_forbidden_userland_state(&fields);
    if contains_userland_state {
        return Err(PersistentFeedbackRejectReason::UserlandState);
    }

    let key = PersistentFeedbackKey {
        source_fingerprint: required(&fields, "source")?.to_owned(),
        engine_version: required(&fields, "engine")?.to_owned(),
        php_target_version: required(&fields, "php")?.to_owned(),
        compile_options: required(&fields, "compile")?.to_owned(),
        function_id: parse_u32(required(&fields, "function")?)?,
        ir_fingerprint: required(&fields, "ir")?.to_owned(),
        instruction_id: parse_u32(required(&fields, "instruction")?)?,
        epochs: PersistentFeedbackEpochs {
            class_table: parse_u64(required(&fields, "class_epoch")?)?,
            function_table: parse_u64(required(&fields, "function_epoch")?)?,
            autoload: parse_u64(required(&fields, "autoload_epoch")?)?,
            include_path: parse_u64(required(&fields, "include_epoch")?)?,
        },
        target_arch_config: required(&fields, "target")?.to_owned(),
    };
    let guard_failures = PersistentGuardFailureSummary {
        failures: parse_u64(fields.get("guard_failures").copied().unwrap_or("0"))?,
        blacklisted: parse_bool(fields.get("blacklisted").copied().unwrap_or("false"))?,
    };
    let payload = PersistentFeedbackPayload {
        callsite_state: parse_callsite_state(required(&fields, "state")?)?,
        scalar_kinds: parse_scalar_kinds(fields.get("scalars").copied().unwrap_or("unknown"))?,
        array_layout: parse_optional_array_layout(fields.get("array_layout").copied())?,
        array_key_shape: parse_optional_array_key_shape(fields.get("array_key").copied())?,
        object_shape: parse_object_shape(&fields)?,
        branch_bias: parse_optional_branch_bias(fields.get("branch").copied())?,
        include_autoload: parse_include_autoload(&fields)?,
        guard_failures,
        quickening: parse_quickening_site(
            &fields,
            key.function_id,
            key.instruction_id,
            guard_failures.failures,
        )?,
        function_callsite: parse_function_callsite(&fields, key.function_id, key.instruction_id)?,
    };

    Ok(PersistentFeedbackEntry {
        key,
        payload,
        contains_userland_state,
    })
}

fn contains_forbidden_userland_state(fields: &BTreeMap<&str, &str>) -> bool {
    for forbidden in [
        "userland_value",
        "object_handle",
        "array_value",
        "resource_handle",
        "global",
        "superglobal",
        "output_buffer",
        "session",
    ] {
        if fields
            .get(forbidden)
            .is_some_and(|value| !matches!(*value, "" | "none" | "false"))
        {
            return true;
        }
    }
    fields.get("request_string").is_some_and(|value| {
        !matches!(
            *value,
            "" | "none" | "false" | "interned" | "engine_immutable"
        )
    })
}

fn required<'a>(
    fields: &'a BTreeMap<&str, &str>,
    name: &str,
) -> Result<&'a str, PersistentFeedbackRejectReason> {
    fields
        .get(name)
        .copied()
        .filter(|value| !value.is_empty())
        .ok_or(PersistentFeedbackRejectReason::Corrupt)
}

fn parse_u32(value: &str) -> Result<u32, PersistentFeedbackRejectReason> {
    value
        .parse()
        .map_err(|_| PersistentFeedbackRejectReason::Corrupt)
}

fn parse_u64(value: &str) -> Result<u64, PersistentFeedbackRejectReason> {
    value
        .parse()
        .map_err(|_| PersistentFeedbackRejectReason::Corrupt)
}

fn parse_bool(value: &str) -> Result<bool, PersistentFeedbackRejectReason> {
    match value {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(PersistentFeedbackRejectReason::Corrupt),
    }
}

fn parse_callsite_state(
    value: &str,
) -> Result<PersistentCallsiteState, PersistentFeedbackRejectReason> {
    match value {
        "cold" => Ok(PersistentCallsiteState::Cold),
        "monomorphic" => Ok(PersistentCallsiteState::Monomorphic),
        "polymorphic" => Ok(PersistentCallsiteState::Polymorphic),
        "megamorphic" => Ok(PersistentCallsiteState::Megamorphic),
        "blacklisted" => Ok(PersistentCallsiteState::Blacklisted),
        _ => Err(PersistentFeedbackRejectReason::Corrupt),
    }
}

fn parse_scalar_kinds(
    value: &str,
) -> Result<Vec<PersistentScalarKind>, PersistentFeedbackRejectReason> {
    if value == "none" || value.is_empty() {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(|scalar| match scalar {
            "null" => Ok(PersistentScalarKind::Null),
            "bool" => Ok(PersistentScalarKind::Bool),
            "int" => Ok(PersistentScalarKind::Int),
            "float" => Ok(PersistentScalarKind::Float),
            "string" => Ok(PersistentScalarKind::String),
            "numeric_string" => Ok(PersistentScalarKind::NumericString),
            "array" => Ok(PersistentScalarKind::Array),
            "object" => Ok(PersistentScalarKind::Object),
            "resource" => Ok(PersistentScalarKind::Resource),
            "unknown" => Ok(PersistentScalarKind::Unknown),
            _ => Err(PersistentFeedbackRejectReason::Corrupt),
        })
        .collect()
}

fn parse_optional_array_layout(
    value: Option<&str>,
) -> Result<Option<PersistentArrayLayout>, PersistentFeedbackRejectReason> {
    value
        .map(|value| match value {
            "empty" => Ok(PersistentArrayLayout::Empty),
            "packed" => Ok(PersistentArrayLayout::Packed),
            "mixed" => Ok(PersistentArrayLayout::Mixed),
            "unknown" => Ok(PersistentArrayLayout::Unknown),
            _ => Err(PersistentFeedbackRejectReason::Corrupt),
        })
        .transpose()
}

fn parse_optional_array_key_shape(
    value: Option<&str>,
) -> Result<Option<PersistentArrayKeyShape>, PersistentFeedbackRejectReason> {
    value
        .map(|value| match value {
            "int" => Ok(PersistentArrayKeyShape::IntOnly),
            "string" => Ok(PersistentArrayKeyShape::StringOnly),
            "int_string" => Ok(PersistentArrayKeyShape::IntString),
            "unknown" => Ok(PersistentArrayKeyShape::Unknown),
            _ => Err(PersistentFeedbackRejectReason::Corrupt),
        })
        .transpose()
}

fn parse_object_shape(
    fields: &BTreeMap<&str, &str>,
) -> Result<Option<PersistentObjectShapeObservation>, PersistentFeedbackRejectReason> {
    let Some(class_id) = fields.get("object_class_id").copied() else {
        return Ok(None);
    };
    let layout_epoch = required(fields, "object_layout_epoch")?;
    let property_slot = match fields.get("property_slot").copied() {
        Some("none") | None => None,
        Some(value) => Some(parse_u32(value)?),
    };
    Ok(Some(PersistentObjectShapeObservation {
        class_id: parse_u32(class_id)?,
        layout_epoch: parse_u64(layout_epoch)?,
        property_slot,
    }))
}

fn parse_optional_branch_bias(
    value: Option<&str>,
) -> Result<Option<PersistentBranchBias>, PersistentFeedbackRejectReason> {
    value
        .map(|value| match value {
            "unknown" => Ok(PersistentBranchBias::Unknown),
            "mostly_true" => Ok(PersistentBranchBias::MostlyTrue),
            "mostly_false" => Ok(PersistentBranchBias::MostlyFalse),
            "balanced" => Ok(PersistentBranchBias::Balanced),
            _ => Err(PersistentFeedbackRejectReason::Corrupt),
        })
        .transpose()
}

/// Parses a monomorphic function-call IC site from `site=ic_function_call`
/// entries. Any missing or malformed field rejects the entry as corrupt.
fn parse_function_callsite(
    fields: &BTreeMap<&str, &str>,
    function_id: u32,
    instruction_id: u32,
) -> Result<Option<crate::inline_cache::FunctionCallSiteSnapshot>, PersistentFeedbackRejectReason> {
    if fields.get("site").copied() != Some("ic_function_call") {
        return Ok(None);
    }
    // Parse the u32-range fields strictly: an out-of-range value is corrupt,
    // never silently truncated into a different valid id. `arity` is
    // additionally capped so a corrupt entry cannot force a huge allocation
    // when the seeder builds the by-ref shape vector.
    let arity = parse_u32(required(fields, "call_arity")?)?;
    if arity > MAX_PERSISTED_CALL_ARITY {
        return Err(PersistentFeedbackRejectReason::Corrupt);
    }
    Ok(Some(crate::inline_cache::FunctionCallSiteSnapshot {
        function: function_id,
        block: parse_u32(required(fields, "ic_block")?)?,
        instruction: instruction_id,
        lowered_name: required(fields, "call_name")?.to_owned(),
        arity,
        epoch: parse_u64(required(fields, "call_site_epoch")?)?,
        target_function: parse_u32(required(fields, "call_target_function")?)?,
    }))
}

fn parse_quickening_site(
    fields: &BTreeMap<&str, &str>,
    function: u32,
    instruction: u32,
    guard_failures: u64,
) -> Result<Option<QuickeningSiteSnapshot>, PersistentFeedbackRejectReason> {
    let Some(site_kind) = fields.get("site").copied() else {
        return Ok(None);
    };
    let site = match site_kind {
        "ir" => QuickeningSiteKey::Ir {
            function,
            block: parse_u32(required(fields, "block")?)?,
            instruction,
        },
        "dense" => QuickeningSiteKey::Dense {
            unit: parse_u32(required(fields, "dense_unit")?)?,
            function,
            instruction,
        },
        // Non-quickening site kinds (inline-cache callsites) are parsed by
        // their own payload parsers.
        "ic_function_call" => return Ok(None),
        _ => return Err(PersistentFeedbackRejectReason::Corrupt),
    };
    let (state, specialization) = match required(fields, "quickening_state")? {
        "specialized" => {
            let specialization = match required(fields, "specialization")? {
                "add_int_int" => QuickeningSpecialization::AddIntInt,
                "sub_int_int" => QuickeningSpecialization::SubIntInt,
                "mul_int_int" => QuickeningSpecialization::MulIntInt,
                "concat_string_string" => QuickeningSpecialization::ConcatStringString,
                "packed_array_int_key" => QuickeningSpecialization::PackedArrayIntKey,
                "bool_branch_condition" => QuickeningSpecialization::BoolBranchCondition,
                _ => return Err(PersistentFeedbackRejectReason::Corrupt),
            };
            (QuickeningState::Specialized, Some(specialization))
        }
        "blacklisted" => (QuickeningState::Blacklisted, None),
        _ => return Err(PersistentFeedbackRejectReason::Corrupt),
    };
    Ok(Some(QuickeningSiteSnapshot {
        site,
        state,
        specialization,
        guard_failures,
    }))
}

fn parse_include_autoload(
    fields: &BTreeMap<&str, &str>,
) -> Result<Option<PersistentIncludeAutoloadStability>, PersistentFeedbackRejectReason> {
    let Some(stable) = fields.get("include_autoload_stable").copied() else {
        return Ok(None);
    };
    Ok(Some(PersistentIncludeAutoloadStability {
        stable: parse_bool(stable)?,
        target_fingerprint: fields
            .get("include_autoload_target")
            .copied()
            .filter(|value| !value.is_empty() && *value != "none")
            .map(ToOwned::to_owned),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> PersistentFeedbackContext {
        PersistentFeedbackContext::new(
            "source-1",
            "engine-1",
            "8.5.7",
            "opt=2,exec=auto",
            "ir-1",
            PersistentFeedbackEpochs {
                class_table: 1,
                function_table: 2,
                autoload: 3,
                include_path: 4,
            },
            "test-target",
        )
    }

    fn valid_entry() -> String {
        format!(
            "{PERSISTENT_FEEDBACK_FORMAT_VERSION}\n\
             entry source=source-1 engine=engine-1 php=8.5.7 compile=opt=2,exec=auto \
             function=0 ir=ir-1 instruction=7 class_epoch=1 function_epoch=2 \
             autoload_epoch=3 include_epoch=4 target=test-target state=monomorphic \
             scalars=int,numeric_string array_layout=packed array_key=int \
             branch=mostly_true guard_failures=0 blacklisted=false\n"
        )
    }

    #[test]
    fn accepts_metadata_only_feedback() {
        let report = context().validate_bytes(valid_entry().as_bytes());

        assert_eq!(report.stats.entries_seen, 1);
        assert_eq!(report.stats.entries_accepted, 1);
        assert_eq!(report.stats.rejected_stale, 0);
        assert!(!report.stats.fallback_to_baseline);
        assert_eq!(report.store.entries().len(), 1);
        assert_eq!(
            report.store.entries()[0].payload.callsite_state,
            PersistentCallsiteState::Monomorphic
        );
    }

    #[test]
    fn stale_source_feedback_falls_back_to_baseline() {
        let text = valid_entry().replace("source=source-1", "source=old");
        let report = context().validate_bytes(text.as_bytes());

        assert_eq!(report.stats.entries_seen, 1);
        assert_eq!(report.stats.entries_accepted, 0);
        assert_eq!(report.stats.rejected_stale, 1);
        assert!(report.stats.fallback_to_baseline);
    }

    #[test]
    fn corrupt_feedback_falls_back_to_baseline() {
        let report = context().validate_bytes(b"not-a-feedback-file\nentry");

        assert_eq!(report.stats.rejected_corrupt, 1);
        assert_eq!(report.stats.entries_accepted, 0);
        assert!(report.stats.fallback_to_baseline);
    }

    #[test]
    fn userland_state_is_rejected() {
        let text = valid_entry().replace("blacklisted=false", "blacklisted=false object_handle=9");
        let report = context().validate_bytes(text.as_bytes());

        assert_eq!(report.stats.rejected_userland_state, 1);
        assert_eq!(report.stats.entries_accepted, 0);
        assert!(report.stats.fallback_to_baseline);
    }

    #[test]
    fn render_sites_roundtrips_through_validation() {
        let context = context();
        let sites = vec![
            QuickeningSiteSnapshot {
                site: QuickeningSiteKey::Dense {
                    unit: 0,
                    function: 3,
                    instruction: 17,
                },
                state: QuickeningState::Specialized,
                specialization: Some(QuickeningSpecialization::AddIntInt),
                guard_failures: 0,
            },
            QuickeningSiteSnapshot {
                site: QuickeningSiteKey::Ir {
                    function: 1,
                    block: 2,
                    instruction: 4,
                },
                state: QuickeningState::Specialized,
                specialization: Some(QuickeningSpecialization::PackedArrayIntKey),
                guard_failures: 1,
            },
            QuickeningSiteSnapshot {
                site: QuickeningSiteKey::Dense {
                    unit: 1,
                    function: 0,
                    instruction: 9,
                },
                state: QuickeningState::Blacklisted,
                specialization: None,
                guard_failures: 4,
            },
        ];

        let text = context.render_sites(&sites);
        let report = context.validate_bytes(text.as_bytes());

        assert_eq!(report.stats.entries_seen, 3);
        assert_eq!(report.stats.entries_accepted, 3);
        assert!(!report.stats.fallback_to_baseline);
        let seeded: Vec<QuickeningSiteSnapshot> = report
            .store
            .entries()
            .iter()
            .filter_map(|entry| entry.payload.quickening)
            .collect();
        assert_eq!(seeded, sites);
    }

    #[test]
    fn corrupt_quickening_fields_are_rejected() {
        let text = format!(
            "{}\nentry source=source-1 engine=engine-1 php=8.5.7 compile=opt=2,exec=auto \
             function=0 ir=ir-1 instruction=7 class_epoch=1 function_epoch=2 \
             autoload_epoch=3 include_epoch=4 target=test-target state=monomorphic \
             site=dense dense_unit=0 quickening_state=specialized specialization=bogus",
            PERSISTENT_FEEDBACK_FORMAT_VERSION
        );
        let report = context().validate_bytes(text.as_bytes());

        assert_eq!(report.stats.rejected_corrupt, 1);
        assert_eq!(report.stats.entries_accepted, 0);
    }

    #[test]
    fn stats_json_is_stable_and_outside_php_stdout() {
        let json = context()
            .validate_bytes(valid_entry().as_bytes())
            .stats
            .to_json();

        assert!(json.contains("\"schema_version\": 3"));
        assert!(json.contains("\"advisory_only\": true"));
        assert!(json.contains("\"consume_mode\": \"off\""));
        assert!(json.contains("\"default_enabled\": false"));
        assert!(json.contains("\"entries_accepted\": 1"));
        assert!(json.contains("\"entries_written\": 0"));
        assert!(json.contains("\"rejected_epoch_mismatch\": 0"));
        assert!(json.contains("\"rejected_architecture_mismatch\": 0"));
        assert!(json.contains("\"rejected_config_mismatch\": 0"));
    }

    #[test]
    fn epoch_mismatch_is_attributed_distinctly_from_stale() {
        let text = valid_entry().replace("class_epoch=1", "class_epoch=99");
        let report = context().validate_bytes(text.as_bytes());

        assert_eq!(report.stats.entries_accepted, 0);
        assert_eq!(report.stats.rejected_epoch_mismatch, 1);
        assert_eq!(report.stats.rejected_stale, 0);
        assert!(report.stats.fallback_to_baseline);
    }

    #[test]
    fn deferred_epoch_validation_accepts_recorded_epochs_for_consumers() {
        // A cold-start load cannot know this run's final epochs; the recorded
        // observation epochs are kept on the entry for consumers to
        // re-validate against live state. Fingerprint mismatches still reject.
        let text = valid_entry().replace("class_epoch=1", "class_epoch=99");
        let report = context()
            .with_epoch_validation(PersistentFeedbackEpochValidation::DeferToConsumption)
            .validate_bytes(text.as_bytes());

        assert_eq!(report.stats.entries_accepted, 1);
        assert_eq!(report.stats.rejected_epoch_mismatch, 0);
        assert_eq!(report.store.entries()[0].key.epochs.class_table, 99);

        let stale = valid_entry().replace("source=source-1", "source=source-2");
        let report = context()
            .with_epoch_validation(PersistentFeedbackEpochValidation::DeferToConsumption)
            .validate_bytes(stale.as_bytes());
        assert_eq!(report.stats.entries_accepted, 0);
        assert_eq!(report.stats.rejected_stale, 1);
    }

    #[test]
    fn function_callsite_entries_roundtrip_through_render_and_validate() {
        let callsite = crate::inline_cache::FunctionCallSiteSnapshot {
            function: 0,
            block: 2,
            instruction: 7,
            lowered_name: "app\\helpers\\format_row".to_owned(),
            arity: 2,
            epoch: 5,
            target_function: 9,
        };
        let (text, written) =
            context().render_feedback_counted(&[], std::slice::from_ref(&callsite));
        assert_eq!(written, 1);
        assert!(text.contains("site=ic_function_call"), "{text}");

        let report = context().validate_bytes(text.as_bytes());
        assert_eq!(report.stats.entries_accepted, 1, "{:?}", report.stats);
        let entry = &report.store.entries()[0];
        assert_eq!(entry.payload.function_callsite.as_ref(), Some(&callsite));
    }

    #[test]
    fn function_callsite_out_of_range_or_huge_arity_is_rejected_not_truncated() {
        // A u32-overflowing target id must reject as corrupt, never wrap into
        // a different valid function id.
        let overflow = text_with_callsite_field("call_target_function", "4294967305");
        let report = context().validate_bytes(overflow.as_bytes());
        assert_eq!(report.stats.entries_accepted, 0, "{:?}", report.stats);
        assert_eq!(report.stats.rejected_corrupt, 1);

        // An absurd arity must reject rather than survive to force a huge
        // allocation in the seeder.
        let huge_arity = text_with_callsite_field("call_arity", "4294967295");
        let report = context().validate_bytes(huge_arity.as_bytes());
        assert_eq!(report.stats.entries_accepted, 0, "{:?}", report.stats);
        assert_eq!(report.stats.rejected_corrupt, 1);
    }

    fn text_with_callsite_field(field: &str, value: &str) -> String {
        let callsite = crate::inline_cache::FunctionCallSiteSnapshot {
            function: 0,
            block: 2,
            instruction: 7,
            lowered_name: "probe".to_owned(),
            arity: 7,
            epoch: 1,
            target_function: 3,
        };
        let (text, _) = context().render_feedback_counted(&[], std::slice::from_ref(&callsite));
        let base = match field {
            "call_arity" => "call_arity=7",
            "call_target_function" => "call_target_function=3",
            other => panic!("unhandled field {other}"),
        };
        let replaced = text.replacen(base, &format!("{field}={value}"), 1);
        assert_ne!(replaced, text, "field {field} not found to replace");
        replaced
    }

    #[test]
    fn writer_stamps_context_epochs_on_entries() {
        let exported = crate::quickening::QuickeningSiteSnapshot {
            site: crate::quickening::QuickeningSiteKey::Dense {
                unit: 0,
                function: 3,
                instruction: 7,
            },
            state: crate::quickening::QuickeningState::Specialized,
            specialization: Some(crate::quickening::QuickeningSpecialization::AddIntInt),
            guard_failures: 0,
        };
        let (text, written) = context()
            .with_epochs(PersistentFeedbackEpochs {
                class_table: 11,
                function_table: 12,
                autoload: 13,
                include_path: 14,
            })
            .render_sites_counted(&[exported]);
        assert_eq!(written, 1);
        assert!(
            text.contains("class_epoch=11 function_epoch=12 autoload_epoch=13 include_epoch=14"),
            "{text}"
        );
    }

    #[test]
    fn architecture_mismatch_is_attributed_distinctly() {
        let text = valid_entry().replace("target=test-target", "target=other-arch");
        let report = context().validate_bytes(text.as_bytes());

        assert_eq!(report.stats.entries_accepted, 0);
        assert_eq!(report.stats.rejected_architecture_mismatch, 1);
        assert_eq!(report.stats.rejected_stale, 0);
        assert_eq!(report.stats.rejected_epoch_mismatch, 0);
    }

    #[test]
    fn config_mismatch_is_attributed_distinctly() {
        let text = valid_entry().replace("compile=opt=2,exec=auto", "compile=opt=0,exec=baseline");
        let report = context().validate_bytes(text.as_bytes());

        assert_eq!(report.stats.entries_accepted, 0);
        assert_eq!(report.stats.rejected_config_mismatch, 1);
        assert_eq!(report.stats.rejected_stale, 0);
    }

    #[test]
    fn globals_superglobals_sessions_and_output_buffers_are_rejected() {
        for forbidden in [
            "global=x",
            "superglobal=_GET",
            "session=abc",
            "output_buffer=1",
        ] {
            let text = valid_entry().replace(
                "blacklisted=false",
                &format!("blacklisted=false {forbidden}"),
            );
            let report = context().validate_bytes(text.as_bytes());
            assert_eq!(
                report.stats.rejected_userland_state, 1,
                "{forbidden} must be rejected as userland state"
            );
            assert_eq!(report.stats.entries_accepted, 0, "{forbidden}");
        }
    }

    #[test]
    fn render_sites_counted_reports_written_entries() {
        let context = context();
        let sites = vec![
            QuickeningSiteSnapshot {
                site: QuickeningSiteKey::Dense {
                    unit: 0,
                    function: 3,
                    instruction: 17,
                },
                state: QuickeningState::Specialized,
                specialization: Some(QuickeningSpecialization::AddIntInt),
                guard_failures: 0,
            },
            // Observing sites are not persisted and must not be counted.
            QuickeningSiteSnapshot {
                site: QuickeningSiteKey::Ir {
                    function: 1,
                    block: 2,
                    instruction: 4,
                },
                state: QuickeningState::Observing,
                specialization: None,
                guard_failures: 0,
            },
        ];

        let (text, written) = context.render_sites_counted(&sites);
        assert_eq!(written, 1, "only the specialized site is written");
        let report = context.validate_bytes(text.as_bytes());
        assert_eq!(report.stats.entries_accepted, written);
    }
}
