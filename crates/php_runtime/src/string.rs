//! Byte-oriented PHP string representation.
//!
//! The backing storage carries two request-invisible caches next to the
//! bytes: a lazily computed stable hash and an optional interned symbol
//! identity for names/literals. Both live inside the shared storage so a
//! `PhpString` handle stays one pointer wide (`Value` must stay 24 bytes)
//! and identity travels with the storage across handle clones. Equality
//! stays byte-exact: symbol identity and the cached hash are shortcuts,
//! never the definition.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;

/// Thread-local interned identity for a name or literal byte sequence.
///
/// Symbol ids are only comparable within one thread; `PhpString` is `!Send`,
/// so two strings that can meet each other always share one interner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SymbolId(u32);

impl SymbolId {
    /// Raw id for diagnostics and tests.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// FNV-1a 64-bit over the exact bytes; deterministic within a process.
/// `0` is reserved as the "not yet computed" sentinel in the cache cell.
fn stable_hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    if hash == 0 { 1 } else { hash }
}

thread_local! {
    static SYMBOL_INTERNER: RefCell<SymbolInterner> = RefCell::new(SymbolInterner::default());
}

#[derive(Default)]
struct SymbolInterner {
    map: HashMap<Vec<u8>, PhpString>,
    next: u32,
    /// Total interned name bytes owned by this interner. Tracked so the engine
    /// can account the persistent immutable-name heap without walking the map.
    total_bytes: u64,
}

impl SymbolInterner {
    fn intern(&mut self, bytes: &[u8]) -> PhpString {
        if let Some(existing) = self.map.get(bytes) {
            crate::layout_stats::record_symbol_intern_hit();
            return existing.clone();
        }
        crate::layout_stats::record_symbol_intern_miss();
        let symbol = SymbolId(self.next);
        self.next = self.next.wrapping_add(1);
        let string = PhpString::from_bytes(bytes.to_vec());
        string.storage.set_symbol(Some(u64::from(symbol.0)));
        self.total_bytes = self.total_bytes.saturating_add(bytes.len() as u64);
        self.map.insert(bytes.to_vec(), string.clone());
        string
    }
}

/// Snapshot of the process-thread-local immutable-name interner as
/// `(entries, bytes)`.
///
/// This interner is the one concrete persistent immutable engine-metadata heap
/// that already survives across requests: it owns only interned immutable name
/// bytes (never userland `Value`s, handles, arrays, resources, or request
/// strings) and is never reset per request. Exposing its footprint lets the VM
/// account `persistent_engine_allocations`/`persistent_engine_bytes` for a
/// class that is proven immutable and engine-owned.
#[must_use]
pub fn symbol_interner_footprint() -> (u64, u64) {
    SYMBOL_INTERNER.with(|interner| {
        let interner = interner.borrow();
        (interner.map.len() as u64, interner.total_bytes)
    })
}

/// PHP string bytes without an implicit UTF-8 invariant.
pub struct PhpString {
    storage: crate::runtime_memory::CompactBytes,
}

impl Default for PhpString {
    fn default() -> Self {
        Self {
            storage: crate::runtime_memory::CompactBytes::from_slice(&[]),
        }
    }
}

impl Clone for PhpString {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
        }
    }
}

/// Byte-exact equality with identity shortcuts: shared storage and paired
/// symbol ids decide without touching the bytes.
impl PartialEq for PhpString {
    fn eq(&self, other: &Self) -> bool {
        if crate::runtime_memory::CompactBytes::ptr_eq(&self.storage, &other.storage) {
            return true;
        }
        if let (Some(lhs), Some(rhs)) = (self.symbol_id(), other.symbol_id()) {
            crate::layout_stats::record_symbol_eq_fast_hit();
            return lhs == rhs;
        }
        crate::layout_stats::record_symbol_eq_byte_fallback();
        self.storage.as_bytes() == other.storage.as_bytes()
    }
}

impl Eq for PhpString {}

/// Hashing uses the cached stable hash so repeated map operations on the
/// same storage hash the bytes once.
impl std::hash::Hash for PhpString {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.stable_hash());
    }
}

impl PhpString {
    /// Creates a PHP string from raw bytes.
    #[must_use]
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        crate::layout_stats::record_string_allocation();
        Self {
            storage: crate::runtime_memory::CompactBytes::from_slice(&bytes.into()),
        }
    }

    /// Creates a PHP string holding the concatenation of `parts` in a
    /// single allocation, with no intermediate growable buffer. This is
    /// the concatenation fast path: the joined length is known up front,
    /// so each part is copied straight into its final position.
    #[must_use]
    pub fn from_parts(parts: &[&[u8]]) -> Self {
        crate::layout_stats::record_string_allocation();
        Self {
            storage: crate::runtime_memory::CompactBytes::from_parts(parts),
        }
    }

    /// Returns the thread-local interned string for these bytes, creating
    /// the symbol on first use. Interned strings share storage, carry a
    /// [`SymbolId`], and compare/hash without touching the bytes.
    #[must_use]
    pub fn intern(bytes: &[u8]) -> Self {
        SYMBOL_INTERNER.with(|interner| interner.borrow_mut().intern(bytes))
    }

    /// Convenience constructor for tests and ASCII literals.
    #[must_use]
    pub fn from_test_str(text: &str) -> Self {
        Self::from_bytes(text.as_bytes().to_vec())
    }

    /// Returns the exact underlying bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.storage.as_bytes()
    }

    /// Returns the interned identity, when this storage was interned and
    /// has not been separated for mutation since.
    #[must_use]
    pub fn symbol_id(&self) -> Option<SymbolId> {
        self.storage.symbol().map(|raw| SymbolId(raw as u32))
    }

    /// Cheap equality: paired symbols or shared storage decide instantly;
    /// otherwise falls back to byte comparison.
    #[must_use]
    pub fn same_symbol_or_bytes(&self, other: &Self) -> bool {
        self == other
    }

    /// Returns the cached stable hash, computing it on first use.
    #[must_use]
    pub fn stable_hash(&self) -> u64 {
        let cached = self.storage.hash();
        if cached != 0 {
            crate::layout_stats::record_string_hash_cache_hit();
            return cached;
        }
        crate::layout_stats::record_string_hash_cache_miss();
        let hash = stable_hash_bytes(self.storage.as_bytes());
        self.storage.set_hash(hash);
        hash
    }

    /// Returns true when this string shares storage with at least one clone.
    #[must_use]
    pub fn is_shared(&self) -> bool {
        !self.storage.is_unique()
    }

    /// Returns true when two PHP strings share the same byte storage.
    #[must_use]
    pub fn shares_storage_with(&self, other: &Self) -> bool {
        crate::runtime_memory::CompactBytes::ptr_eq(&self.storage, &other.storage)
    }

    /// Ensures this string has unique storage before byte mutation.
    ///
    /// Normal PHP assignment clones the `PhpString` handle and shares the
    /// underlying bytes. Mutation must call this boundary first so writes do
    /// not leak into by-value copies. The cached hash and symbol identity
    /// are dropped here: the bytes are about to change.
    pub fn separate_for_write(&mut self) {
        if self.is_shared() {
            crate::layout_stats::record_cow_separation();
            self.storage = crate::runtime_memory::CompactBytes::from_slice(self.storage.as_bytes());
        }
        self.storage.set_hash(0);
        self.storage.set_symbol(None);
    }

    /// Returns mutable, fixed-length bytes after copy-on-write separation.
    /// Growth happens by rebuilding through [`PhpString::into_bytes`] and
    /// [`PhpString::from_bytes`]; storage length is immutable.
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        self.separate_for_write();
        self.storage.unique_bytes_mut()
    }

    /// Consumes the string and returns the exact bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.storage.as_bytes().to_vec()
    }

    /// Returns true when the string has no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    /// Returns the byte length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.storage.len()
    }

    /// Returns a process-local identity for the shared byte storage.
    ///
    /// This is only suitable for request-local caches that also validate the
    /// current bytes. It is not a PHP-visible identity.
    #[must_use]
    pub fn native_storage_id(&self) -> usize {
        self.storage.addr()
    }

    pub(crate) fn storage_id(&self) -> usize {
        self.native_storage_id()
    }

    /// Test/debug convenience for non-runtime display.
    #[must_use]
    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(self.storage.as_bytes()).into_owned()
    }
}

impl From<Vec<u8>> for PhpString {
    fn from(bytes: Vec<u8>) -> Self {
        Self::from_bytes(bytes)
    }
}

impl From<&[u8]> for PhpString {
    fn from(bytes: &[u8]) -> Self {
        Self::from_bytes(bytes.to_vec())
    }
}

impl From<&str> for PhpString {
    fn from(text: &str) -> Self {
        Self::from_test_str(text)
    }
}

impl fmt::Debug for PhpString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PhpString")
            .field("bytes", &self.storage.as_bytes())
            .field("lossy", &self.to_string_lossy())
            .finish()
    }
}

impl fmt::Display for PhpString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use super::PhpString;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    #[test]
    fn symbol_interner_footprint_grows_with_new_names_and_ignores_repeats() {
        use super::symbol_interner_footprint;
        let (before_count, before_bytes) = symbol_interner_footprint();
        let name = format!("phrust_p4_footprint_probe_{}", std::process::id());
        let _first = PhpString::intern(name.as_bytes());
        let (after_count, after_bytes) = symbol_interner_footprint();
        assert!(
            after_count > before_count,
            "interning a new immutable name grows the persistent heap count"
        );
        assert!(
            after_bytes >= before_bytes + name.len() as u64,
            "persistent heap bytes include the new name"
        );
        // Re-interning the same name is a hit and must not grow the footprint.
        let _repeat = PhpString::intern(name.as_bytes());
        let (repeat_count, repeat_bytes) = symbol_interner_footprint();
        assert_eq!(repeat_count, after_count);
        assert_eq!(repeat_bytes, after_bytes);
    }

    fn std_hash(value: &PhpString) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn cow_string_assignment_shares_until_write() {
        let original = PhpString::from("abc");
        let mut copy = original.clone();

        assert!(original.is_shared());
        assert!(copy.is_shared());

        copy.bytes_mut()[1] = b'Z';

        assert_eq!(original.as_bytes(), b"abc");
        assert_eq!(copy.as_bytes(), b"aZc");
        assert!(!copy.is_shared());
    }

    #[test]
    fn equal_bytes_with_different_storage_compare_equal() {
        let a = PhpString::from_bytes(b"same".to_vec());
        let b = PhpString::from_bytes(b"same".to_vec());
        assert!(!a.shares_storage_with(&b));
        assert_eq!(a, b);
        assert_eq!(std_hash(&a), std_hash(&b));
        assert!(a.same_symbol_or_bytes(&b));
    }

    #[test]
    fn binary_strings_hash_and_compare_by_exact_bytes() {
        let a = PhpString::from_bytes(vec![0xff, 0x00, 0xfe]);
        let b = PhpString::from_bytes(vec![0xff, 0x00, 0xfe]);
        let c = PhpString::from_bytes(vec![0xff, 0x00, 0xff]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(std_hash(&a), std_hash(&b));
        assert_eq!(a.stable_hash(), b.stable_hash());
        assert_ne!(a.stable_hash(), c.stable_hash());
    }

    #[test]
    fn interned_identifiers_share_storage_and_symbol() {
        let a = PhpString::intern(b"interned_identifier_equality");
        let b = PhpString::intern(b"interned_identifier_equality");
        assert!(a.shares_storage_with(&b));
        assert_eq!(a.symbol_id(), b.symbol_id());
        assert!(a.symbol_id().is_some());
        assert_eq!(a, b);

        let other = PhpString::intern(b"interned_identifier_other");
        assert_ne!(a.symbol_id(), other.symbol_id());
        assert_ne!(a, other);

        // Uninterned equal bytes still compare equal (byte fallback).
        let plain = PhpString::from_bytes(b"interned_identifier_equality".to_vec());
        assert!(plain.symbol_id().is_none());
        assert_eq!(a, plain);
        assert_eq!(std_hash(&a), std_hash(&plain));
    }

    #[test]
    fn mutation_after_cow_drops_symbol_and_hash() {
        let interned = PhpString::intern(b"mutation_drops_identity");
        let hash_before = interned.stable_hash();
        let mut copy = interned.clone();
        // Growth rebuilds the storage; identity semantics match in-place
        // mutation (hash and symbol drop with the bytes change).
        let mut grown = copy.into_bytes();
        grown.push(b'!');
        copy = PhpString::from_bytes(grown);

        assert!(copy.symbol_id().is_none(), "mutated copy keeps no symbol");
        assert_eq!(
            interned.symbol_id().map(|_| ()),
            Some(()),
            "original keeps its symbol"
        );
        assert_eq!(interned.stable_hash(), hash_before);
        assert_ne!(copy.stable_hash(), hash_before);
        assert_ne!(interned, copy);

        // Unique (unshared) strings must also drop identity on mutation.
        let mut unique = PhpString::intern(b"mutation_unique_case").clone();
        // Drop the interner's handle from the equation: the storage is still
        // shared with the interner map, so separation occurs.
        unique.bytes_mut()[0] = b'M';
        assert!(unique.symbol_id().is_none());
    }

    #[test]
    fn builtin_lookup_resolves_interned_names() {
        let name = PhpString::intern(b"strlen");
        let registry = crate::builtins::BuiltinRegistry::new();
        let text = std::str::from_utf8(name.as_bytes()).expect("ascii builtin name");
        assert!(registry.get(text).is_some());
        assert!(registry.contains(text));
    }

    #[test]
    fn stable_hash_is_computed_once_per_storage() {
        crate::layout_stats::reset_layout_stats();
        let value = PhpString::from_bytes(b"hash_once".to_vec());
        let first = value.stable_hash();
        let clone = value.clone();
        let second = clone.stable_hash();
        assert_eq!(first, second);
        let stats = crate::layout_stats::take_layout_stats();
        assert_eq!(stats.string_hash_cache_misses, 1, "{stats:?}");
        assert!(stats.string_hash_cache_hits >= 1, "{stats:?}");
    }
}
