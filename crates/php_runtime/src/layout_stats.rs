//! Request-local runtime layout and allocation counters.
//!
//! Recording is disabled by default so hot paths (`Value::clone`, string and
//! array handle allocation) pay only one relaxed atomic load plus a predicted
//! branch. The VM enables recording through [`reset_layout_stats`] exactly
//! when counter collection is requested. The enable flag is process-global
//! and sticky: it is never cleared once set, because a concurrent
//! counters-enabled execution in another thread must not lose events. The
//! stats themselves stay thread-local, and every collector resets them before
//! measuring, so stale increments from non-counter executions never leak into
//! a collected snapshot.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

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

static LAYOUT_STATS_ENABLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static LAYOUT_STATS: RefCell<RuntimeLayoutStats> =
        RefCell::new(RuntimeLayoutStats::default());
}

/// Returns true when layout/allocation stats recording is enabled.
#[inline(always)]
pub(crate) fn stats_enabled() -> bool {
    LAYOUT_STATS_ENABLED.load(Ordering::Relaxed)
}

/// Enables stats recording (sticky; see module docs). Shared by the layout
/// and numeric-string stat collectors so either reset path opts in.
pub(crate) fn enable_stats() {
    LAYOUT_STATS_ENABLED.store(true, Ordering::Relaxed);
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

layout_recorder!(pub(crate) record_value_clone, record_value_clone_slow, value_clones);
layout_recorder!(
    pub(crate) record_string_allocation,
    record_string_allocation_slow,
    string_allocations
);
layout_recorder!(
    pub(crate) record_array_handle_clone,
    record_array_handle_clone_slow,
    array_handle_clones
);
layout_recorder!(
    pub(crate) record_cow_separation,
    record_cow_separation_slow,
    cow_separations
);
layout_recorder!(
    pub(crate) record_reference_cell_creation,
    record_reference_cell_creation_slow,
    reference_cell_creations
);
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
/// recording (sticky; see module docs).
pub fn reset_layout_stats() {
    enable_stats();
    LAYOUT_STATS.with(|stats| *stats.borrow_mut() = RuntimeLayoutStats::default());
}

/// Returns and clears layout counters.
#[must_use]
pub fn take_layout_stats() -> RuntimeLayoutStats {
    LAYOUT_STATS.with(|stats| {
        let mut stats = stats.borrow_mut();
        let current = *stats;
        *stats = RuntimeLayoutStats::default();
        current
    })
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
        let mut array_clone = array.clone();
        array_clone.append(Value::Int(3));
        let _cell = ReferenceCell::new(Value::String(string));
        let _value_clone = Value::Array(array).clone();

        let stats = layout_stats::take_layout_stats();
        assert!(stats.value_clones >= 1, "{stats:?}");
        assert!(stats.string_allocations >= 1, "{stats:?}");
        assert!(stats.array_handle_clones >= 2, "{stats:?}");
        assert!(stats.cow_separations >= 1, "{stats:?}");
        assert_eq!(stats.reference_cell_creations, 1);
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
}
