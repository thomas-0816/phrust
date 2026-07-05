//! Request-local runtime layout and allocation counters.
//!
//! Recording is disabled by default so hot paths (`Value::clone`, string and
//! array handle allocation) skip request-local accounting unless the current
//! thread is explicitly collecting counters. The VM enables recording through
//! [`reset_layout_stats`] exactly when counter collection is requested and
//! disables it again when [`take_layout_stats`] ends the collection window.

use std::cell::RefCell;
use std::collections::BTreeMap;

/// Runtime value/layout counters collected by the VM when counters are enabled.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeLayoutStats {
    /// Runtime `Value` clones observed during execution.
    pub value_clones: u64,
    /// PHP byte-string backing allocations.
    pub string_allocations: u64,
    /// PHP array handle clones sharing copy-on-write storage.
    pub array_handle_clones: u64,
    /// Copy-on-write storage separations for runtime containers.
    pub cow_separations: u64,
    /// Reference cells created for PHP references/aliases.
    pub reference_cell_creations: u64,
    /// Runtime object storage allocations.
    pub object_allocations: u64,
    /// Array reads satisfied by direct packed integer indexing.
    pub array_packed_direct_gets: u64,
    /// Array reads satisfied by the mixed-storage key index.
    pub array_mixed_indexed_gets: u64,
    /// Array reads that used a remaining linear fallback path.
    pub array_linear_scan_fallbacks: u64,
    /// Full array metadata recomputes after structural repair.
    pub array_metadata_recomputes: u64,
    /// Compiled-unit symbol lookups satisfied by maps.
    pub symbol_map_lookups: u64,
    /// Compiled-unit symbol lookups that used a linear fallback.
    pub symbol_linear_fallbacks: u64,
    /// String interner lookups that reused an existing symbol.
    pub symbol_intern_hits: u64,
    /// String interner lookups that created a new symbol.
    pub symbol_intern_misses: u64,
    /// String hashes served from the per-storage cache.
    pub string_hash_cache_hits: u64,
    /// String hashes computed and cached.
    pub string_hash_cache_misses: u64,
    /// String equality decided by paired symbol identity.
    pub symbol_eq_fast_hits: u64,
    /// String equality that fell back to byte comparison.
    pub symbol_eq_byte_fallbacks: u64,
    /// Declared object property reads served from the slot vector.
    pub object_declared_slot_reads: u64,
    /// Declared object property writes into the slot vector.
    pub object_declared_slot_writes: u64,
    /// Object property reads that consulted the dynamic side map.
    pub object_dynamic_property_map_reads: u64,
    /// Object property writes into the dynamic side map.
    pub object_dynamic_property_map_writes: u64,
    /// Packed arrays constructed with values-only storage.
    pub packed_values_storage_arrays: u64,
    /// Reads served directly from values-only packed storage.
    pub packed_values_storage_reads: u64,
    /// Appends into values-only packed storage.
    pub packed_values_storage_appends: u64,
    /// Packed iterations that synthesized their integer keys.
    pub packed_virtual_key_iterations: u64,
    /// Packed arrays converted to mixed by a string-key insert.
    pub packed_to_mixed_string_key: u64,
    /// Packed arrays converted to mixed by a non-sequential integer key.
    pub packed_to_mixed_non_sequential_int_key: u64,
    /// Packed arrays converted to mixed by an append after an unset tail.
    pub packed_to_mixed_append_key_gap: u64,
    /// Packed arrays converted to mixed by an unset creating a hole.
    pub packed_to_mixed_unset_hole: u64,
    /// Arrays promoted into record (shaped string-key) storage.
    pub record_storage_arrays: u64,
    /// Record shape promotions (packed/empty to record transitions).
    pub record_shape_promotions: u64,
    /// Reads resolved through a record shape slot.
    pub record_slot_reads: u64,
    /// Writes resolved through a record shape slot.
    pub record_slot_writes: u64,
    /// Record probes whose key carried interned symbol identity.
    pub record_key_symbol_hits: u64,
    /// Record arrays converted to mixed by an integer-key insert.
    pub record_to_mixed_int_key: u64,
    /// Record arrays converted to mixed by an ambiguous string key.
    pub record_to_mixed_ambiguous_key: u64,
    /// Record arrays converted to mixed by unset/order-sensitive mutation.
    pub record_to_mixed_generic_mutation: u64,
}

/// Request-local attribution for layout events whose aggregate counters are
/// not enough to identify the VM source that produced them.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeLayoutSourceStats {
    pub value_clone_by_family: BTreeMap<String, u64>,
    pub array_handle_clone_by_family: BTreeMap<String, u64>,
    pub cow_separation_by_family: BTreeMap<String, u64>,
    pub reference_cell_creation_by_family: BTreeMap<String, u64>,
}

/// Scope guard for request-local layout source attribution.
#[derive(Debug)]
pub struct LayoutSourceGuard {
    active: bool,
}

/// Why a record array had to leave shaped storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordToMixedReason {
    IntKey,
    AmbiguousKey,
    GenericMutation,
}

/// Why a packed array had to leave values-only storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackedToMixedReason {
    StringKey,
    NonSequentialIntKey,
    AppendKeyGap,
    UnsetHole,
}

thread_local! {
    static LAYOUT_STATS_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static LAYOUT_STATS: RefCell<RuntimeLayoutStats> =
        RefCell::new(RuntimeLayoutStats::default());
    static LAYOUT_SOURCE_STATS: RefCell<RuntimeLayoutSourceStats> =
        RefCell::new(RuntimeLayoutSourceStats::default());
    static LAYOUT_SOURCE_STACK: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

const UNATTRIBUTED_SOURCE_FAMILY: &str = "unattributed";

/// Value copied while reading an array element or equivalent array storage.
pub const SOURCE_ARRAY_ELEMENT_READ: &str = "array_element_read";
/// Value copied while writing an array element or equivalent array storage.
pub const SOURCE_ARRAY_ELEMENT_WRITE: &str = "array_element_write";
/// Value copied while materializing output from generic array builtins.
pub const SOURCE_ARRAY_BUILTIN_OUTPUT_MATERIALIZATION: &str =
    "array_builtin_output_materialization";
/// Value copied while binding by-reference storage.
pub const SOURCE_BY_REF_ARGUMENT_BINDING: &str = "by_ref_argument_binding";
/// Value copied while taking a callable argument snapshot.
pub const SOURCE_CALL_ARGUMENT_SNAPSHOT: &str = "call_argument_snapshot";
/// Value copied while binding a closure capture.
pub const SOURCE_CLOSURE_CAPTURE_BINDING: &str = "closure_capture_binding";
/// Value copied while materializing arguments for builtin parameter handling.
pub const SOURCE_BUILTIN_ARGUMENT_MATERIALIZATION: &str = "builtin_argument_materialization";
/// Value copied while materializing foreach-style iteration output.
pub const SOURCE_FOREACH_VALUE: &str = "foreach_value";
/// Value copied while reading object property storage.
pub const SOURCE_OBJECT_PROPERTY_READ: &str = "object_property_read";
/// Value copied while converting output to string storage.
pub const SOURCE_OUTPUT_STRING_CONVERSION: &str = "output_string_conversion";
/// Value copied while dereferencing a PHP reference cell.
pub const SOURCE_REFERENCE_DEREFERENCE: &str = "reference_dereference";
/// Value copied while materializing a return value.
pub const SOURCE_RETURN_VALUE: &str = "return_value";
/// Value copied between stack/register/local storage.
pub const SOURCE_STACK_REGISTER_LOCAL_MOVE: &str = "stack_register_local_move";

/// Returns true when layout/allocation stats recording is enabled.
#[inline(always)]
pub(crate) fn stats_enabled() -> bool {
    LAYOUT_STATS_ENABLED.with(std::cell::Cell::get)
}

/// Enables stats recording for the current thread. Shared by the layout
/// and numeric-string stat collectors so either reset path opts in.
pub(crate) fn enable_stats() {
    LAYOUT_STATS_ENABLED.with(|enabled| enabled.set(true));
}

/// Disables stats recording for the current thread.
pub(crate) fn disable_stats() {
    LAYOUT_STATS_ENABLED.with(|enabled| enabled.set(false));
}

macro_rules! layout_recorder {
    ($vis:vis $name:ident, $slow:ident, $field:ident) => {
        #[inline(always)]
        $vis fn $name() {
            if stats_enabled() {
                $slow();
            }
        }

        #[cold]
        #[inline(never)]
        fn $slow() {
            LAYOUT_STATS.with(|stats| stats.borrow_mut().$field += 1);
        }
    };
}

/// Enters a source family for subsequent layout events in this thread.
#[must_use]
pub fn enter_layout_source_family(family: &'static str) -> LayoutSourceGuard {
    if !stats_enabled() {
        return LayoutSourceGuard { active: false };
    }
    LAYOUT_SOURCE_STACK.with(|stack| stack.borrow_mut().push(family));
    LayoutSourceGuard { active: true }
}

/// Enters a source family only when no more specific source is active.
#[must_use]
pub fn enter_default_layout_source_family(family: &'static str) -> LayoutSourceGuard {
    if !stats_enabled() {
        return LayoutSourceGuard { active: false };
    }
    LAYOUT_SOURCE_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        if stack.is_empty() {
            stack.push(family);
            LayoutSourceGuard { active: true }
        } else {
            LayoutSourceGuard { active: false }
        }
    })
}

impl Drop for LayoutSourceGuard {
    fn drop(&mut self) {
        if self.active {
            LAYOUT_SOURCE_STACK.with(|stack| {
                stack.borrow_mut().pop();
            });
        }
    }
}

fn current_source_family() -> &'static str {
    LAYOUT_SOURCE_STACK.with(|stack| {
        stack
            .borrow()
            .last()
            .copied()
            .unwrap_or(UNATTRIBUTED_SOURCE_FAMILY)
    })
}

fn record_value_clone_source() {
    let family = current_source_family();
    LAYOUT_SOURCE_STATS.with(|stats| {
        *stats
            .borrow_mut()
            .value_clone_by_family
            .entry(family.to_owned())
            .or_default() += 1;
    });
}

fn record_array_handle_clone_source() {
    let family = current_source_family();
    LAYOUT_SOURCE_STATS.with(|stats| {
        *stats
            .borrow_mut()
            .array_handle_clone_by_family
            .entry(family.to_owned())
            .or_default() += 1;
    });
}

fn record_cow_separation_source() {
    let family = current_source_family();
    LAYOUT_SOURCE_STATS.with(|stats| {
        *stats
            .borrow_mut()
            .cow_separation_by_family
            .entry(family.to_owned())
            .or_default() += 1;
    });
}

fn record_reference_cell_creation_source() {
    let family = current_source_family();
    LAYOUT_SOURCE_STATS.with(|stats| {
        *stats
            .borrow_mut()
            .reference_cell_creation_by_family
            .entry(family.to_owned())
            .or_default() += 1;
    });
}

#[inline(always)]
pub(crate) fn record_value_clone() {
    if stats_enabled() {
        record_value_clone_slow();
    }
}

#[cold]
#[inline(never)]
fn record_value_clone_slow() {
    LAYOUT_STATS.with(|stats| stats.borrow_mut().value_clones += 1);
    record_value_clone_source();
}

layout_recorder!(
    pub(crate) record_string_allocation,
    record_string_allocation_slow,
    string_allocations
);

#[inline(always)]
pub(crate) fn record_array_handle_clone() {
    if stats_enabled() {
        record_array_handle_clone_slow();
    }
}

#[cold]
#[inline(never)]
fn record_array_handle_clone_slow() {
    LAYOUT_STATS.with(|stats| stats.borrow_mut().array_handle_clones += 1);
    record_array_handle_clone_source();
}

#[inline(always)]
pub(crate) fn record_cow_separation() {
    if stats_enabled() {
        record_cow_separation_slow();
    }
}

#[cold]
#[inline(never)]
fn record_cow_separation_slow() {
    LAYOUT_STATS.with(|stats| stats.borrow_mut().cow_separations += 1);
    record_cow_separation_source();
}

#[inline(always)]
pub(crate) fn record_reference_cell_creation() {
    if stats_enabled() {
        record_reference_cell_creation_slow();
    }
}

#[cold]
#[inline(never)]
fn record_reference_cell_creation_slow() {
    LAYOUT_STATS.with(|stats| stats.borrow_mut().reference_cell_creations += 1);
    record_reference_cell_creation_source();
}

layout_recorder!(
    pub(crate) record_object_allocation,
    record_object_allocation_slow,
    object_allocations
);
layout_recorder!(
    pub(crate) record_array_packed_direct_get,
    record_array_packed_direct_get_slow,
    array_packed_direct_gets
);
layout_recorder!(
    pub(crate) record_array_mixed_indexed_get,
    record_array_mixed_indexed_get_slow,
    array_mixed_indexed_gets
);
layout_recorder!(
    pub(crate) record_array_linear_scan_fallback,
    record_array_linear_scan_fallback_slow,
    array_linear_scan_fallbacks
);
layout_recorder!(
    pub(crate) record_array_metadata_recompute,
    record_array_metadata_recompute_slow,
    array_metadata_recomputes
);
layout_recorder!(pub record_symbol_map_lookup, record_symbol_map_lookup_slow, symbol_map_lookups);
layout_recorder!(
    pub record_symbol_linear_fallback,
    record_symbol_linear_fallback_slow,
    symbol_linear_fallbacks
);
layout_recorder!(
    pub(crate) record_symbol_intern_hit,
    record_symbol_intern_hit_slow,
    symbol_intern_hits
);
layout_recorder!(
    pub(crate) record_symbol_intern_miss,
    record_symbol_intern_miss_slow,
    symbol_intern_misses
);
layout_recorder!(
    pub(crate) record_string_hash_cache_hit,
    record_string_hash_cache_hit_slow,
    string_hash_cache_hits
);
layout_recorder!(
    pub(crate) record_string_hash_cache_miss,
    record_string_hash_cache_miss_slow,
    string_hash_cache_misses
);
layout_recorder!(
    pub(crate) record_symbol_eq_fast_hit,
    record_symbol_eq_fast_hit_slow,
    symbol_eq_fast_hits
);
layout_recorder!(
    pub(crate) record_symbol_eq_byte_fallback,
    record_symbol_eq_byte_fallback_slow,
    symbol_eq_byte_fallbacks
);
layout_recorder!(
    pub(crate) record_object_declared_slot_read,
    record_object_declared_slot_read_slow,
    object_declared_slot_reads
);
layout_recorder!(
    pub(crate) record_object_declared_slot_write,
    record_object_declared_slot_write_slow,
    object_declared_slot_writes
);
layout_recorder!(
    pub(crate) record_object_dynamic_property_map_read,
    record_object_dynamic_property_map_read_slow,
    object_dynamic_property_map_reads
);
layout_recorder!(
    pub(crate) record_object_dynamic_property_map_write,
    record_object_dynamic_property_map_write_slow,
    object_dynamic_property_map_writes
);
layout_recorder!(
    pub(crate) record_packed_values_storage_array,
    record_packed_values_storage_array_slow,
    packed_values_storage_arrays
);
layout_recorder!(
    pub(crate) record_packed_values_storage_read,
    record_packed_values_storage_read_slow,
    packed_values_storage_reads
);
layout_recorder!(
    pub(crate) record_packed_values_storage_append,
    record_packed_values_storage_append_slow,
    packed_values_storage_appends
);
layout_recorder!(
    pub(crate) record_packed_virtual_key_iteration,
    record_packed_virtual_key_iteration_slow,
    packed_virtual_key_iterations
);

layout_recorder!(
    pub(crate) record_record_storage_array,
    record_record_storage_array_slow,
    record_storage_arrays
);
layout_recorder!(
    pub(crate) record_record_shape_promotion,
    record_record_shape_promotion_slow,
    record_shape_promotions
);
layout_recorder!(
    pub(crate) record_record_slot_read,
    record_record_slot_read_slow,
    record_slot_reads
);
layout_recorder!(
    pub(crate) record_record_slot_write,
    record_record_slot_write_slow,
    record_slot_writes
);
layout_recorder!(
    pub(crate) record_record_key_symbol_hit,
    record_record_key_symbol_hit_slow,
    record_key_symbol_hits
);

/// Reason-tagged record-to-mixed conversion recorder.
#[inline(always)]
pub(crate) fn record_record_to_mixed(reason: RecordToMixedReason) {
    if stats_enabled() {
        record_record_to_mixed_slow(reason);
    }
}

#[cold]
#[inline(never)]
fn record_record_to_mixed_slow(reason: RecordToMixedReason) {
    LAYOUT_STATS.with(|stats| {
        let mut stats = stats.borrow_mut();
        match reason {
            RecordToMixedReason::IntKey => stats.record_to_mixed_int_key += 1,
            RecordToMixedReason::AmbiguousKey => stats.record_to_mixed_ambiguous_key += 1,
            RecordToMixedReason::GenericMutation => {
                stats.record_to_mixed_generic_mutation += 1;
            }
        }
    });
}

/// Reason-tagged packed-to-mixed conversion recorder.
#[inline(always)]
pub(crate) fn record_packed_to_mixed(reason: PackedToMixedReason) {
    if stats_enabled() {
        record_packed_to_mixed_slow(reason);
    }
}

#[cold]
#[inline(never)]
fn record_packed_to_mixed_slow(reason: PackedToMixedReason) {
    LAYOUT_STATS.with(|stats| {
        let mut stats = stats.borrow_mut();
        match reason {
            PackedToMixedReason::StringKey => stats.packed_to_mixed_string_key += 1,
            PackedToMixedReason::NonSequentialIntKey => {
                stats.packed_to_mixed_non_sequential_int_key += 1;
            }
            PackedToMixedReason::AppendKeyGap => stats.packed_to_mixed_append_key_gap += 1,
            PackedToMixedReason::UnsetHole => stats.packed_to_mixed_unset_hole += 1,
        }
    });
}

/// Clears layout counters for deterministic VM executions and enables
/// recording for the current thread.
pub fn reset_layout_stats() {
    enable_stats();
    LAYOUT_STATS.with(|stats| *stats.borrow_mut() = RuntimeLayoutStats::default());
    LAYOUT_SOURCE_STATS.with(|stats| *stats.borrow_mut() = RuntimeLayoutSourceStats::default());
    LAYOUT_SOURCE_STACK.with(|stack| stack.borrow_mut().clear());
}

/// Returns and clears layout counters.
#[must_use]
pub fn take_layout_stats() -> RuntimeLayoutStats {
    disable_stats();
    LAYOUT_STATS.with(|stats| {
        let mut stats = stats.borrow_mut();
        let current = *stats;
        *stats = RuntimeLayoutStats::default();
        current
    })
}

/// Returns and clears source-attributed layout counters.
#[must_use]
pub fn take_layout_source_stats() -> RuntimeLayoutSourceStats {
    disable_stats();
    LAYOUT_SOURCE_STATS.with(|stats| std::mem::take(&mut *stats.borrow_mut()))
}

#[cfg(test)]
mod tests {
    use crate::{PhpArray, PhpString, ReferenceCell, Value, layout_stats};

    #[test]
    fn layout_stats_record_safe_runtime_events() {
        layout_stats::reset_layout_stats();

        let string = PhpString::from("abc");
        let _string_clone = string.clone();
        let array = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2)]);
        {
            let _source = layout_stats::enter_layout_source_family("test_array");
            let mut array_clone = array.clone();
            array_clone.append(Value::Int(3));
        }
        {
            let _source = layout_stats::enter_layout_source_family("test_reference");
            let _cell = ReferenceCell::new(Value::String(string));
        }
        {
            let _source = layout_stats::enter_layout_source_family("test_value");
            let _value_clone = Value::Array(array).clone();
        }

        let stats = layout_stats::take_layout_stats();
        assert!(stats.value_clones >= 1, "{stats:?}");
        assert!(stats.string_allocations >= 1, "{stats:?}");
        assert!(stats.array_handle_clones >= 2, "{stats:?}");
        assert!(stats.cow_separations >= 1, "{stats:?}");
        assert_eq!(stats.reference_cell_creations, 1);

        let source_stats = layout_stats::take_layout_source_stats();
        assert!(
            source_stats
                .value_clone_by_family
                .get("test_value")
                .copied()
                .unwrap_or_default()
                >= 1,
            "{source_stats:?}"
        );
        assert!(
            source_stats
                .array_handle_clone_by_family
                .get("test_array")
                .copied()
                .unwrap_or_default()
                >= 1,
            "{source_stats:?}"
        );
        assert_eq!(
            source_stats
                .reference_cell_creation_by_family
                .get("test_reference"),
            Some(&1)
        );
    }

    #[test]
    fn layout_stats_record_array_and_symbol_hot_paths() {
        layout_stats::reset_layout_stats();

        layout_stats::record_array_packed_direct_get();
        layout_stats::record_array_mixed_indexed_get();
        layout_stats::record_array_linear_scan_fallback();
        layout_stats::record_array_metadata_recompute();
        layout_stats::record_symbol_map_lookup();
        layout_stats::record_symbol_linear_fallback();

        let stats = layout_stats::take_layout_stats();
        assert_eq!(stats.array_packed_direct_gets, 1);
        assert_eq!(stats.array_mixed_indexed_gets, 1);
        assert_eq!(stats.array_linear_scan_fallbacks, 1);
        assert_eq!(stats.array_metadata_recomputes, 1);
        assert_eq!(stats.symbol_map_lookups, 1);
        assert_eq!(stats.symbol_linear_fallbacks, 1);
    }

    #[test]
    fn taking_layout_stats_disables_later_hot_path_recording() {
        layout_stats::reset_layout_stats();
        layout_stats::record_array_packed_direct_get();
        assert_eq!(
            layout_stats::take_layout_stats().array_packed_direct_gets,
            1
        );

        layout_stats::record_array_packed_direct_get();
        assert_eq!(
            layout_stats::take_layout_stats().array_packed_direct_gets,
            0
        );
    }

    #[test]
    fn taking_source_stats_disables_later_hot_path_recording() {
        layout_stats::reset_layout_stats();
        {
            let _source = layout_stats::enter_layout_source_family("test_value");
            let _value_clone = Value::Int(1).clone();
        }
        let source_stats = layout_stats::take_layout_source_stats();
        assert_eq!(
            source_stats.value_clone_by_family.get("test_value"),
            Some(&1)
        );

        layout_stats::record_array_packed_direct_get();
        assert_eq!(
            layout_stats::take_layout_stats().array_packed_direct_gets,
            0
        );
    }

    #[test]
    fn default_source_scopes_classify_runtime_clone_sites() {
        layout_stats::reset_layout_stats();

        let cell = ReferenceCell::new(Value::Int(1));
        let _value = cell.get();

        let slot = crate::ValueSlot::value(Value::Array(PhpArray::new()));
        let _value = slot.read();

        let array = PhpArray::from_packed(vec![Value::Array(PhpArray::new())]);
        let _pair = array.pair_at(0).expect("array pair");

        let source_stats = layout_stats::take_layout_source_stats();
        assert_eq!(
            source_stats
                .value_clone_by_family
                .get(layout_stats::SOURCE_REFERENCE_DEREFERENCE),
            Some(&1)
        );
        assert!(
            source_stats
                .value_clone_by_family
                .get(layout_stats::SOURCE_STACK_REGISTER_LOCAL_MOVE)
                .copied()
                .unwrap_or_default()
                >= 1,
            "{source_stats:?}"
        );
        assert!(
            source_stats
                .array_handle_clone_by_family
                .get(layout_stats::SOURCE_STACK_REGISTER_LOCAL_MOVE)
                .copied()
                .unwrap_or_default()
                >= 1,
            "{source_stats:?}"
        );
        assert_eq!(
            source_stats
                .value_clone_by_family
                .get(layout_stats::SOURCE_FOREACH_VALUE),
            Some(&1)
        );
        assert_eq!(
            source_stats
                .array_handle_clone_by_family
                .get(layout_stats::SOURCE_FOREACH_VALUE),
            Some(&1)
        );
        assert!(
            !source_stats
                .value_clone_by_family
                .contains_key("unattributed"),
            "{source_stats:?}"
        );
        assert!(
            !source_stats
                .array_handle_clone_by_family
                .contains_key("unattributed"),
            "{source_stats:?}"
        );
    }
}
