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
