//! Opaque ordered PHP array storage for runtime-semantics.

use crate::{
    PhpString, Value,
    numeric_string::{
        NumericStringArrayKey, array_key_has_numeric_string_ambiguity, classify_array_key,
    },
};
use std::rc::{Rc, Weak};

/// PHP array key after runtime-semantics key normalization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArrayKey {
    /// Integer array key.
    Int(i64),
    /// String array key.
    String(PhpString),
}

impl ArrayKey {
    /// Converts a runtime value into a runtime-semantics PHP array key.
    ///
    /// Supported conversions:
    /// - `int` remains an integer key;
    /// - `bool` becomes `0` or `1`;
    /// - `null` becomes the empty-string key;
    /// - `float` truncates toward zero;
    /// - decimal integer strings without a leading plus and without leading
    ///   zeroes become integer keys;
    /// - all other strings remain string keys.
    #[must_use]
    pub fn from_value_mvp(value: &Value) -> Option<Self> {
        match value {
            Value::Int(value) => Some(Self::Int(*value)),
            Value::Bool(false) => Some(Self::Int(0)),
            Value::Bool(true) => Some(Self::Int(1)),
            Value::Null => Some(Self::String(PhpString::from_bytes(Vec::new()))),
            Value::Float(value) => Some(Self::Int(value.to_f64() as i64)),
            Value::String(value) => Some(Self::from_php_string(value.clone())),
            Value::Uninitialized => Some(Self::String(PhpString::from_bytes(Vec::new()))),
            Value::Array(_)
            | Value::Object(_)
            | Value::Resource(_)
            | Value::Fiber(_)
            | Value::Generator(_)
            | Value::Callable(_)
            | Value::Reference(_) => None,
        }
    }

    /// Normalizes a PHP string key in the tested MVP range.
    #[must_use]
    pub fn from_php_string(value: PhpString) -> Self {
        match classify_array_key(&value) {
            NumericStringArrayKey::Integer(key) => Self::Int(key),
            NumericStringArrayKey::String => Self::String(value),
        }
    }

    /// Returns the integer key when present.
    #[must_use]
    pub const fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(*value),
            Self::String(_) => None,
        }
    }

    /// Returns the string key when present.
    #[must_use]
    pub const fn as_string(&self) -> Option<&PhpString> {
        match self {
            Self::String(value) => Some(value),
            Self::Int(_) => None,
        }
    }
}

/// Runtime array storage kind proven by array metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhpArrayKind {
    /// Integer keys are exactly `0..len` in insertion order.
    PackedList,
    /// Any shape outside the packed-list invariant.
    MixedHash,
}

/// Cheap direct-element summary for guarded array fast paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhpArrayElementSummary {
    /// The array has no elements.
    Empty,
    /// Every direct element is an integer value.
    AllInt,
    /// At least one direct element is not an integer.
    Mixed,
}

/// Cheap key-shape summary for guarded array fast paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhpArrayKeyKindSummary {
    /// The array has no keys.
    Empty,
    /// Integer keys are exactly `0..len` in insertion order.
    SequentialInt,
    /// All keys are integers, but not a packed sequential list.
    IntOnly,
    /// All keys are strings.
    StringOnly,
    /// Both integer and string keys are present.
    Mixed,
}

/// Coarse PHP array shape observed for guarded lookup policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhpArrayShapeKind {
    Empty,
    Packed,
    PackedWithHoles,
    SmallInlineMap,
    InternedStringKeyRecord,
    ShapeStableRecordLike,
    MixedHash,
    SharedImmutableLiteralArray,
    CowOrReferenceFallback,
}

impl PhpArrayShapeKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Packed => "packed",
            Self::PackedWithHoles => "packed_with_holes",
            Self::SmallInlineMap => "small_inline_map",
            Self::InternedStringKeyRecord => "interned_string_key_record",
            Self::ShapeStableRecordLike => "shape_stable_record_like",
            Self::MixedHash => "mixed_hash",
            Self::SharedImmutableLiteralArray => "shared_immutable_literal_array",
            Self::CowOrReferenceFallback => "cow_or_reference_fallback",
        }
    }
}

/// Metadata snapshot for non-mutating array-shape observers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhpArrayShapeMetadata {
    pub kind: PhpArrayShapeKind,
    pub len: usize,
    pub mutation_epoch: u64,
    pub is_shared: bool,
    pub contains_references: bool,
    pub key_kind_summary: PhpArrayKeyKindSummary,
    pub numeric_string_key_ambiguity: bool,
}

/// Conservative reason a shape lookup must use the generic array path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhpArrayShapeLookupFallback {
    KeyCoercion,
    OrderSemantics,
    CowOrReference,
    UnsupportedShape,
}

impl PhpArrayShapeLookupFallback {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KeyCoercion => "key_coercion",
            Self::OrderSemantics => "order_semantics",
            Self::CowOrReference => "cow_or_reference",
            Self::UnsupportedShape => "unsupported_shape",
        }
    }
}

/// Result of an exact guarded array-shape lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhpArrayShapeLookup<'a> {
    Hit(&'a Value),
    Miss,
    Fallback(PhpArrayShapeLookupFallback),
}

/// Guard metadata for packed-array fast paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhpArrayPackedMetadata {
    pub kind: PhpArrayKind,
    pub element_summary: PhpArrayElementSummary,
    pub is_shared: bool,
    pub contains_references: bool,
    pub mutation_epoch: u64,
    pub packed_len: Option<usize>,
    pub key_kind_summary: PhpArrayKeyKindSummary,
    pub numeric_string_key_ambiguity: bool,
}

/// Conservative packed-int reduction guard failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhpArrayPackedIntReductionError {
    /// The array is mixed or has a hole.
    NotPacked,
    /// The array is shared through copy-on-write storage.
    Shared,
    /// The array contains direct reference cells.
    ContainsReferences,
    /// At least one element is not an integer.
    NonIntElement,
    /// Integer addition overflowed.
    Overflow,
}

/// One ordered array slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArrayEntry {
    key: ArrayKey,
    value: Value,
}

impl ArrayEntry {
    /// Array key.
    #[must_use]
    pub const fn key(&self) -> &ArrayKey {
        &self.key
    }

    /// Array value.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }
}

/// Packed array storage.
///
/// The packed variant keeps full `ArrayEntry` slots for now so the public
/// insertion-order iterator can continue yielding borrowed keys. The variant
/// boundary is still useful: packed metadata is structural instead of inferred
/// from a mixed hash side table, and future values-only storage can stay behind
/// `PhpArray`.
#[derive(Clone, Debug)]
struct PackedArrayStorage {
    entries: Vec<ArrayEntry>,
    next_append_key: Option<i64>,
    internal_pointer: Option<usize>,
    mutation_epoch: u64,
}

/// Mixed array storage for holes, string keys, and non-sequential integer keys.
#[derive(Clone, Debug)]
struct MixedArrayStorage {
    entries: Vec<ArrayEntry>,
    next_append_key: Option<i64>,
    internal_pointer: Option<usize>,
    mutation_epoch: u64,
}

/// Ordered PHP array storage.
///
/// The storage is intentionally opaque. Callers interact through key/value APIs
/// and shape metadata, not through packed or mixed internals.
#[derive(Clone, Debug)]
enum ArrayStorage {
    Packed(PackedArrayStorage),
    Mixed(MixedArrayStorage),
}

impl Default for ArrayStorage {
    fn default() -> Self {
        Self::Packed(PackedArrayStorage {
            entries: Vec::new(),
            next_append_key: None,
            internal_pointer: None,
            mutation_epoch: 0,
        })
    }
}

impl PartialEq for ArrayStorage {
    fn eq(&self, other: &Self) -> bool {
        self.entries() == other.entries()
            && self.next_append_key() == other.next_append_key()
            && self.internal_pointer() == other.internal_pointer()
    }
}

impl Eq for ArrayStorage {}

impl ArrayStorage {
    fn entries(&self) -> &[ArrayEntry] {
        match self {
            Self::Packed(storage) => &storage.entries,
            Self::Mixed(storage) => &storage.entries,
        }
    }

    fn entries_mut(&mut self) -> &mut Vec<ArrayEntry> {
        match self {
            Self::Packed(storage) => &mut storage.entries,
            Self::Mixed(storage) => &mut storage.entries,
        }
    }

    fn next_append_key(&self) -> Option<i64> {
        match self {
            Self::Packed(storage) => storage.next_append_key,
            Self::Mixed(storage) => storage.next_append_key,
        }
    }

    fn set_next_append_key(&mut self, value: Option<i64>) {
        match self {
            Self::Packed(storage) => storage.next_append_key = value,
            Self::Mixed(storage) => storage.next_append_key = value,
        }
    }

    fn internal_pointer(&self) -> Option<usize> {
        match self {
            Self::Packed(storage) => storage.internal_pointer,
            Self::Mixed(storage) => storage.internal_pointer,
        }
    }

    fn set_internal_pointer(&mut self, value: Option<usize>) {
        match self {
            Self::Packed(storage) => storage.internal_pointer = value,
            Self::Mixed(storage) => storage.internal_pointer = value,
        }
    }

    fn mutation_epoch(&self) -> u64 {
        match self {
            Self::Packed(storage) => storage.mutation_epoch,
            Self::Mixed(storage) => storage.mutation_epoch,
        }
    }

    fn set_mutation_epoch(&mut self, value: u64) {
        match self {
            Self::Packed(storage) => storage.mutation_epoch = value,
            Self::Mixed(storage) => storage.mutation_epoch = value,
        }
    }

    fn len(&self) -> usize {
        self.entries().len()
    }

    fn is_empty(&self) -> bool {
        self.entries().is_empty()
    }

    fn is_packed(&self) -> bool {
        matches!(self, Self::Packed(_))
    }

    fn make_mixed(&mut self) {
        let Self::Packed(storage) = self else {
            return;
        };
        let mixed = MixedArrayStorage {
            entries: std::mem::take(&mut storage.entries),
            next_append_key: storage.next_append_key,
            internal_pointer: storage.internal_pointer,
            mutation_epoch: storage.mutation_epoch,
        };
        *self = Self::Mixed(mixed);
    }
}

/// Copy-on-write ordered PHP array facade.
///
/// Cloning a `PhpArray` shares immutable storage. Mutating methods call
/// `separate_for_write` through `storage_mut`, so by-value assignment shares
/// until the first write while true PHP references still write through their
/// owning slot/reference cell.
#[derive(Debug, Eq, PartialEq)]
pub struct PhpArray {
    storage: Rc<ArrayStorage>,
}

impl Default for PhpArray {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for PhpArray {
    fn clone(&self) -> Self {
        crate::layout_stats::record_array_handle_clone();
        Self {
            storage: Rc::clone(&self.storage),
        }
    }
}

/// Weak debug handle to array storage for GC tests.
#[derive(Clone, Debug)]
pub struct WeakArrayHandle {
    id: usize,
    storage: Weak<ArrayStorage>,
}

impl WeakArrayHandle {
    /// Returns the process-local debug ID for this handle.
    #[must_use]
    pub const fn id(&self) -> usize {
        self.id
    }

    /// Returns true when the array storage is still alive.
    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.storage.strong_count() > 0
    }
}

impl PhpArray {
    /// Creates an empty array.
    #[must_use]
    pub fn new() -> Self {
        Self {
            storage: Rc::new(ArrayStorage::Packed(PackedArrayStorage {
                entries: Vec::new(),
                next_append_key: None,
                internal_pointer: None,
                mutation_epoch: 0,
            })),
        }
    }

    /// Creates a packed array with integer keys starting at zero.
    #[must_use]
    pub fn from_packed(elements: Vec<Value>) -> Self {
        let len = elements.len();
        let entries = elements
            .into_iter()
            .enumerate()
            .map(|(index, value)| ArrayEntry {
                key: ArrayKey::Int(index as i64),
                value,
            })
            .collect::<Vec<_>>();
        Self {
            storage: Rc::new(ArrayStorage::Packed(PackedArrayStorage {
                entries,
                next_append_key: (len > 0).then(|| i64::try_from(len).ok()).flatten(),
                internal_pointer: (len > 0).then_some(0),
                mutation_epoch: len as u64,
            })),
        }
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.storage.len()
    }

    /// Returns true when no entries are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    /// Returns true when this array shares storage with at least one clone.
    #[must_use]
    pub fn is_shared(&self) -> bool {
        Rc::strong_count(&self.storage) > 1
    }

    /// Returns true when tracked metadata proves the array is exactly
    /// `0..len` in insertion order.
    #[must_use]
    pub fn is_packed_fast(&self) -> bool {
        self.storage.is_packed()
    }

    /// Returns the packed length when tracked metadata proves packed storage.
    #[must_use]
    pub fn packed_len_fast(&self) -> Option<usize> {
        self.is_packed_fast().then_some(self.storage.len())
    }

    /// Returns the array kind proven by tracked metadata.
    #[must_use]
    pub fn kind_fast(&self) -> PhpArrayKind {
        if self.is_packed_fast() {
            PhpArrayKind::PackedList
        } else {
            PhpArrayKind::MixedHash
        }
    }

    /// Returns true when any direct array slot stores a PHP reference.
    #[must_use]
    pub fn contains_references_fast(&self) -> bool {
        self.storage
            .entries()
            .iter()
            .any(|entry| matches!(entry.value, Value::Reference(_)))
    }

    /// Returns a cheap direct-element summary.
    #[must_use]
    pub fn element_summary_fast(&self) -> PhpArrayElementSummary {
        if self.storage.is_empty() {
            return PhpArrayElementSummary::Empty;
        }
        if self
            .storage
            .entries()
            .iter()
            .all(|entry| matches!(entry.value, Value::Int(_)))
        {
            PhpArrayElementSummary::AllInt
        } else {
            PhpArrayElementSummary::Mixed
        }
    }

    /// Returns a cheap key-shape summary.
    #[must_use]
    pub fn key_kind_summary_fast(&self) -> PhpArrayKeyKindSummary {
        if self.storage.is_empty() {
            return PhpArrayKeyKindSummary::Empty;
        }
        if self.is_packed_fast() {
            return PhpArrayKeyKindSummary::SequentialInt;
        }
        let has_int = self
            .storage
            .entries()
            .iter()
            .any(|entry| matches!(entry.key, ArrayKey::Int(_)));
        let has_string = self
            .storage
            .entries()
            .iter()
            .any(|entry| matches!(entry.key, ArrayKey::String(_)));
        match (has_int, has_string) {
            (true, false) => PhpArrayKeyKindSummary::IntOnly,
            (false, true) => PhpArrayKeyKindSummary::StringOnly,
            (true, true) => PhpArrayKeyKindSummary::Mixed,
            (false, false) => PhpArrayKeyKindSummary::Empty,
        }
    }

    /// Returns true when string keys look numeric but intentionally remain
    /// strings under PHP key-normalization rules.
    #[must_use]
    pub fn has_numeric_string_key_ambiguity_fast(&self) -> bool {
        self.storage.entries().iter().any(|entry| {
            matches!(
                &entry.key,
                ArrayKey::String(key) if array_key_has_numeric_string_ambiguity(key)
            )
        })
    }

    /// Returns the current structural/content mutation epoch.
    #[must_use]
    pub fn mutation_epoch(&self) -> u64 {
        self.storage.mutation_epoch()
    }

    /// Returns packed-array guard metadata for VM and JIT consumers.
    #[must_use]
    pub fn packed_metadata(&self) -> PhpArrayPackedMetadata {
        PhpArrayPackedMetadata {
            kind: self.kind_fast(),
            element_summary: self.element_summary_fast(),
            is_shared: self.is_shared(),
            contains_references: self.contains_references_fast(),
            mutation_epoch: self.mutation_epoch(),
            packed_len: self.packed_len_fast(),
            key_kind_summary: self.key_kind_summary_fast(),
            numeric_string_key_ambiguity: self.has_numeric_string_key_ambiguity_fast(),
        }
    }

    /// Returns conservative shape metadata for non-packed lookup observers.
    #[must_use]
    pub fn shape_metadata(&self) -> PhpArrayShapeMetadata {
        let key_kind_summary = self.key_kind_summary_fast();
        let contains_references = self.contains_references_fast();
        let numeric_string_key_ambiguity = self.has_numeric_string_key_ambiguity_fast();
        let kind = if contains_references {
            PhpArrayShapeKind::CowOrReferenceFallback
        } else if self.is_empty() {
            PhpArrayShapeKind::Empty
        } else if self.is_packed_fast() {
            PhpArrayShapeKind::Packed
        } else if key_kind_summary == PhpArrayKeyKindSummary::StringOnly
            && !numeric_string_key_ambiguity
            && self.string_keys_share_storage_fast()
        {
            PhpArrayShapeKind::InternedStringKeyRecord
        } else if key_kind_summary == PhpArrayKeyKindSummary::StringOnly
            && !numeric_string_key_ambiguity
        {
            PhpArrayShapeKind::ShapeStableRecordLike
        } else if key_kind_summary == PhpArrayKeyKindSummary::IntOnly {
            PhpArrayShapeKind::PackedWithHoles
        } else if self.len() <= 4 && !numeric_string_key_ambiguity {
            PhpArrayShapeKind::SmallInlineMap
        } else if self.is_shared() {
            PhpArrayShapeKind::SharedImmutableLiteralArray
        } else {
            PhpArrayShapeKind::MixedHash
        };

        PhpArrayShapeMetadata {
            kind,
            len: self.len(),
            mutation_epoch: self.mutation_epoch(),
            is_shared: self.is_shared(),
            contains_references,
            key_kind_summary,
            numeric_string_key_ambiguity,
        }
    }

    /// Exact guarded read for string-key record-like arrays.
    #[must_use]
    pub fn record_shape_string_key_lookup(&self, key: &ArrayKey) -> PhpArrayShapeLookup<'_> {
        let ArrayKey::String(_) = key else {
            return PhpArrayShapeLookup::Fallback(PhpArrayShapeLookupFallback::KeyCoercion);
        };
        let metadata = self.shape_metadata();
        if metadata.contains_references {
            return PhpArrayShapeLookup::Fallback(PhpArrayShapeLookupFallback::CowOrReference);
        }
        if metadata.numeric_string_key_ambiguity {
            return PhpArrayShapeLookup::Fallback(PhpArrayShapeLookupFallback::KeyCoercion);
        }
        if !matches!(
            metadata.kind,
            PhpArrayShapeKind::InternedStringKeyRecord | PhpArrayShapeKind::ShapeStableRecordLike
        ) {
            return PhpArrayShapeLookup::Fallback(PhpArrayShapeLookupFallback::UnsupportedShape);
        }
        self.get(key)
            .map_or(PhpArrayShapeLookup::Miss, PhpArrayShapeLookup::Hit)
    }

    /// Exact guarded read for very small non-packed maps.
    #[must_use]
    pub fn small_map_lookup(&self, key: &ArrayKey) -> PhpArrayShapeLookup<'_> {
        let metadata = self.shape_metadata();
        if metadata.contains_references {
            return PhpArrayShapeLookup::Fallback(PhpArrayShapeLookupFallback::CowOrReference);
        }
        if metadata.numeric_string_key_ambiguity {
            return PhpArrayShapeLookup::Fallback(PhpArrayShapeLookupFallback::KeyCoercion);
        }
        if metadata.kind != PhpArrayShapeKind::SmallInlineMap {
            return PhpArrayShapeLookup::Fallback(PhpArrayShapeLookupFallback::UnsupportedShape);
        }
        self.get(key)
            .map_or(PhpArrayShapeLookup::Miss, PhpArrayShapeLookup::Hit)
    }

    fn string_keys_share_storage_fast(&self) -> bool {
        self.storage.entries().iter().all(|entry| {
            matches!(
                &entry.key,
                ArrayKey::String(key) if key.is_shared()
            )
        })
    }

    /// Sums a packed all-int array only when COW/reference/overflow guards pass.
    pub fn packed_int_sum_fast(&self) -> Result<i64, PhpArrayPackedIntReductionError> {
        let metadata = self.packed_metadata();
        if metadata.kind != PhpArrayKind::PackedList {
            return Err(PhpArrayPackedIntReductionError::NotPacked);
        }
        if metadata.is_shared {
            return Err(PhpArrayPackedIntReductionError::Shared);
        }
        if metadata.contains_references {
            return Err(PhpArrayPackedIntReductionError::ContainsReferences);
        }
        match metadata.element_summary {
            PhpArrayElementSummary::Empty => return Ok(0),
            PhpArrayElementSummary::AllInt => {}
            PhpArrayElementSummary::Mixed => {
                return Err(PhpArrayPackedIntReductionError::NonIntElement);
            }
        }

        let mut sum = 0i64;
        for (_, value) in self.iter() {
            let Value::Int(value) = value else {
                return Err(PhpArrayPackedIntReductionError::NonIntElement);
            };
            sum = sum
                .checked_add(*value)
                .ok_or(PhpArrayPackedIntReductionError::Overflow)?;
        }
        Ok(sum)
    }

    /// Returns a process-local storage identity for GC debug snapshots.
    ///
    /// This is not a PHP-visible handle and must only be used by runtime tests
    /// and diagnostics.
    #[must_use]
    pub fn gc_debug_id(&self) -> usize {
        Rc::as_ptr(&self.storage).cast::<()>() as usize
    }

    /// Returns the current `Rc` strong count for GC debug metadata.
    #[must_use]
    pub fn gc_refcount_estimate(&self) -> usize {
        Rc::strong_count(&self.storage)
    }

    /// Returns a weak debug handle for GC tests.
    #[must_use]
    pub fn weak_handle(&self) -> WeakArrayHandle {
        WeakArrayHandle {
            id: self.gc_debug_id(),
            storage: Rc::downgrade(&self.storage),
        }
    }

    /// Ensures this array has unique storage before mutation.
    pub fn separate_for_write(&mut self) {
        let _ = self.storage_mut();
    }

    /// Inserts or overwrites a key. Existing-key overwrites preserve insertion
    /// order.
    pub fn insert(&mut self, key: ArrayKey, value: Value) -> Option<Value> {
        let storage = self.storage_mut();
        bump_append_key(storage, &key);
        if let Some(index) = storage.entries().iter().position(|entry| entry.key == key) {
            bump_mutation_epoch(storage);
            return Some(std::mem::replace(
                &mut storage.entries_mut()[index].value,
                value,
            ));
        }
        let old_len = storage.len();
        let remains_packed =
            storage.is_packed() && matches!(key, ArrayKey::Int(value) if value == old_len as i64);
        if !remains_packed {
            storage.make_mixed();
        }
        storage.entries_mut().push(ArrayEntry { key, value });
        if storage.internal_pointer().is_none() {
            storage.set_internal_pointer(Some(0));
        }
        bump_mutation_epoch(storage);
        None
    }

    /// Appends with the next integer key.
    pub fn append(&mut self, value: Value) -> ArrayKey {
        let storage = self.storage_mut();
        let key = ArrayKey::Int(storage.next_append_key().unwrap_or(0));
        let old_len = storage.len();
        let remains_packed =
            storage.is_packed() && matches!(key, ArrayKey::Int(value) if value == old_len as i64);
        bump_append_key(storage, &key);
        if !remains_packed {
            storage.make_mixed();
        }
        storage.entries_mut().push(ArrayEntry {
            key: key.clone(),
            value,
        });
        if storage.internal_pointer().is_none() {
            storage.set_internal_pointer(Some(0));
        }
        bump_mutation_epoch(storage);
        key
    }

    /// Merges array-spread entries into this array using PHP array-unpack
    /// semantics: integer keys append/reindex, string keys overwrite.
    pub fn spread_extend(&mut self, source: &Self) {
        for (key, value) in source.iter() {
            match key {
                ArrayKey::Int(_) => {
                    self.append(value.clone());
                }
                ArrayKey::String(key) => {
                    self.insert(ArrayKey::String(key.clone()), value.clone());
                }
            }
        }
    }

    /// Returns a value by normalized key.
    #[must_use]
    pub fn get(&self, key: &ArrayKey) -> Option<&Value> {
        self.storage
            .entries()
            .iter()
            .find(|entry| &entry.key == key)
            .map(ArrayEntry::value)
    }

    /// Returns a mutable value by normalized key without exposing storage.
    pub fn get_mut(&mut self, key: &ArrayKey) -> Option<&mut Value> {
        let storage = self.storage_mut();
        if let Some(index) = storage.entries().iter().position(|entry| &entry.key == key) {
            bump_mutation_epoch(storage);
            return Some(&mut storage.entries_mut()[index].value);
        }
        None
    }

    /// Removes a value by normalized key.
    pub fn remove(&mut self, key: &ArrayKey) -> Option<Value> {
        let storage = self.storage_mut();
        storage
            .entries()
            .iter()
            .position(|entry| &entry.key == key)
            .map(|index| {
                let was_packed = storage.is_packed();
                let old_len = storage.len();
                let value = storage.entries_mut().remove(index).value;
                if was_packed && index + 1 != old_len {
                    storage.make_mixed();
                }
                adjust_pointer_after_remove(storage, index);
                bump_mutation_epoch(storage);
                value
            })
    }

    /// Removes and returns the last element, mirroring PHP's `array_pop`
    /// adjustment of the next auto-index: when the removed key is the most
    /// recent auto-index (`next_append_key - 1`), the next index is decremented
    /// so a following `[]=` reuses it (e.g. popping `-2` from `[-2 => x]` makes
    /// the next append `-2` again).
    pub fn pop(&mut self) -> Option<Value> {
        let last_key = self.storage.entries().last()?.key.clone();
        let previous_next = self.storage.next_append_key();
        let value = self.remove(&last_key);
        if let ArrayKey::Int(key) = last_key
            && previous_next == Some(key.saturating_add(1))
        {
            self.storage_mut().set_next_append_key(Some(key));
        }
        value
    }

    /// Returns the current internal-pointer value.
    #[must_use]
    pub fn pointer_value(&self) -> Option<Value> {
        self.storage
            .internal_pointer()
            .and_then(|index| self.storage.entries().get(index))
            .map(ArrayEntry::value)
            .cloned()
    }

    /// Returns the current internal-pointer key.
    #[must_use]
    pub fn pointer_key(&self) -> Option<ArrayKey> {
        self.storage
            .internal_pointer()
            .and_then(|index| self.storage.entries().get(index))
            .map(ArrayEntry::key)
            .cloned()
    }

    /// Moves the internal pointer to the first element.
    pub fn reset_pointer(&mut self) -> Option<Value> {
        let storage = self.storage_mut();
        if storage.is_empty() {
            storage.set_internal_pointer(None);
            return None;
        }
        storage.set_internal_pointer(Some(0));
        storage.entries().first().map(ArrayEntry::value).cloned()
    }

    /// Moves the internal pointer to the last element.
    pub fn end_pointer(&mut self) -> Option<Value> {
        let storage = self.storage_mut();
        let last = storage.len().checked_sub(1)?;
        storage.set_internal_pointer(Some(last));
        storage.entries().get(last).map(ArrayEntry::value).cloned()
    }

    /// Advances the internal pointer by one element.
    pub fn next_pointer(&mut self) -> Option<Value> {
        let storage = self.storage_mut();
        let current = storage.internal_pointer()?;
        let next = current.saturating_add(1);
        if next >= storage.len() {
            storage.set_internal_pointer(None);
            return None;
        }
        storage.set_internal_pointer(Some(next));
        storage.entries().get(next).map(ArrayEntry::value).cloned()
    }

    /// Moves the internal pointer one element backwards.
    pub fn prev_pointer(&mut self) -> Option<Value> {
        let storage = self.storage_mut();
        let Some(current) = storage.internal_pointer() else {
            let last = storage.len().checked_sub(1)?;
            storage.set_internal_pointer(Some(last));
            return storage.entries().get(last).map(ArrayEntry::value).cloned();
        };
        let Some(previous) = current.checked_sub(1) else {
            storage.set_internal_pointer(None);
            return None;
        };
        storage.set_internal_pointer(Some(previous));
        storage
            .entries()
            .get(previous)
            .map(ArrayEntry::value)
            .cloned()
    }

    /// Iterates in insertion order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&ArrayKey, &Value)> {
        self.storage
            .entries()
            .iter()
            .map(|entry| (entry.key(), entry.value()))
    }

    /// Returns packed elements only when the keys are exactly `0..len`.
    #[must_use]
    pub fn packed_elements(&self) -> Option<Vec<&Value>> {
        if self.is_packed_fast() {
            return Some(
                self.storage
                    .entries()
                    .iter()
                    .map(ArrayEntry::value)
                    .collect(),
            );
        }
        let mut elements = Vec::with_capacity(self.storage.len());
        for (index, entry) in self.storage.entries().iter().enumerate() {
            if entry.key != ArrayKey::Int(index as i64) {
                return None;
            }
            elements.push(&entry.value);
        }
        Some(elements)
    }

    /// Returns one packed element only when the keys are exactly `0..len`.
    #[must_use]
    pub fn packed_element(&self, index: usize) -> Option<&Value> {
        for (entry_index, entry) in self.storage.entries().iter().enumerate() {
            if entry.key != ArrayKey::Int(entry_index as i64) {
                return None;
            }
        }
        self.storage.entries().get(index).map(ArrayEntry::value)
    }

    /// Returns one packed element using only tracked metadata.
    #[must_use]
    pub fn packed_element_fast(&self, index: usize) -> Option<&Value> {
        self.is_packed_fast()
            .then(|| self.storage.entries().get(index).map(ArrayEntry::value))
            .flatten()
    }

    fn storage_mut(&mut self) -> &mut ArrayStorage {
        if self.is_shared() {
            crate::layout_stats::record_cow_separation();
        }
        Rc::make_mut(&mut self.storage)
    }
}

fn bump_append_key(storage: &mut ArrayStorage, key: &ArrayKey) {
    if let ArrayKey::Int(value) = key {
        let next = value.saturating_add(1);
        if storage
            .next_append_key()
            .is_none_or(|current| next > current)
        {
            storage.set_next_append_key(Some(next));
        }
    }
}

fn bump_mutation_epoch(storage: &mut ArrayStorage) {
    storage.set_mutation_epoch(storage.mutation_epoch().wrapping_add(1));
}

fn adjust_pointer_after_remove(storage: &mut ArrayStorage, removed_index: usize) {
    let Some(pointer) = storage.internal_pointer() else {
        return;
    };
    let pointer = if storage.is_empty() {
        None
    } else if pointer > removed_index {
        Some(pointer - 1)
    } else if pointer >= storage.len() {
        None
    } else {
        Some(pointer)
    };
    storage.set_internal_pointer(pointer);
}

#[cfg(test)]
mod tests {
    use super::{
        ArrayKey, ArrayStorage, PhpArray, PhpArrayElementSummary, PhpArrayKeyKindSummary,
        PhpArrayKind, PhpArrayPackedIntReductionError, PhpArrayShapeKind, PhpArrayShapeLookup,
        PhpArrayShapeLookupFallback,
    };
    use crate::{PhpString, Value};

    #[test]
    fn array_preserves_insertion_order_and_overwrite_position() {
        let mut array = PhpArray::new();
        array.insert(ArrayKey::String(PhpString::from("a")), Value::Int(1));
        array.insert(ArrayKey::Int(4), Value::Int(2));
        array.insert(ArrayKey::String(PhpString::from("a")), Value::Int(3));

        let entries = array
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>();
        assert_eq!(
            entries,
            vec![
                (ArrayKey::String(PhpString::from("a")), Value::Int(3)),
                (ArrayKey::Int(4), Value::Int(2)),
            ]
        );
    }

    #[test]
    fn array_append_key_tracks_largest_integer_key() {
        let mut array = PhpArray::new();
        assert_eq!(array.append(Value::Int(1)), ArrayKey::Int(0));
        array.insert(ArrayKey::Int(7), Value::Int(2));
        assert_eq!(array.append(Value::Int(3)), ArrayKey::Int(8));
        array.insert(ArrayKey::Int(4), Value::Int(4));
        assert_eq!(array.append(Value::Int(5)), ArrayKey::Int(9));
    }

    #[test]
    fn from_packed_builds_exact_packed_shape() {
        assert_eq!(PhpArray::from_packed(Vec::new()), PhpArray::new());

        let mut array = PhpArray::from_packed(vec![Value::Int(10), Value::Int(20)]);

        assert_eq!(array.len(), 2);
        assert_eq!(array.kind_fast(), PhpArrayKind::PackedList);
        assert_eq!(array.packed_len_fast(), Some(2));
        assert_eq!(array.pointer_key(), Some(ArrayKey::Int(0)));
        assert_eq!(array.append(Value::Int(30)), ArrayKey::Int(2));
        assert_eq!(array.packed_len_fast(), Some(3));
        assert_eq!(array.get(&ArrayKey::Int(2)), Some(&Value::Int(30)));
    }

    #[test]
    fn array_storage_remains_packed_until_shape_requires_mixed() {
        let mut array = PhpArray::new();
        assert!(matches!(array.storage.as_ref(), ArrayStorage::Packed(_)));

        array.append(Value::Int(1));
        array.append(Value::Int(2));
        array.insert(ArrayKey::Int(1), Value::Int(20));
        assert!(matches!(array.storage.as_ref(), ArrayStorage::Packed(_)));
        assert_eq!(array.packed_len_fast(), Some(2));

        array.insert(ArrayKey::String(PhpString::from("name")), Value::Int(3));
        assert!(matches!(array.storage.as_ref(), ArrayStorage::Mixed(_)));
        assert_eq!(
            array.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>(),
            vec![
                ArrayKey::Int(0),
                ArrayKey::Int(1),
                ArrayKey::String(PhpString::from("name")),
            ]
        );
    }

    #[test]
    fn array_storage_converts_after_holes_and_non_reused_append_keys() {
        let mut middle = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(middle.remove(&ArrayKey::Int(1)), Some(Value::Int(2)));
        assert!(matches!(middle.storage.as_ref(), ArrayStorage::Mixed(_)));
        assert_eq!(
            middle
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>(),
            vec![ArrayKey::Int(0), ArrayKey::Int(2)]
        );

        let mut tail = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(tail.pop(), Some(Value::Int(3)));
        assert!(matches!(tail.storage.as_ref(), ArrayStorage::Packed(_)));
        assert_eq!(tail.append(Value::Int(4)), ArrayKey::Int(2));
        assert!(matches!(tail.storage.as_ref(), ArrayStorage::Packed(_)));
        assert_eq!(tail.packed_len_fast(), Some(3));

        let mut unset_tail =
            PhpArray::from_packed(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(unset_tail.remove(&ArrayKey::Int(2)), Some(Value::Int(3)));
        assert!(matches!(
            unset_tail.storage.as_ref(),
            ArrayStorage::Packed(_)
        ));
        assert_eq!(unset_tail.append(Value::Int(4)), ArrayKey::Int(3));
        assert!(matches!(
            unset_tail.storage.as_ref(),
            ArrayStorage::Mixed(_)
        ));
    }

    #[test]
    fn array_append_key_tracks_negative_integer_keys() {
        let mut array = PhpArray::new();
        array.insert(ArrayKey::Int(-5), Value::Int(1));
        assert_eq!(array.append(Value::Int(2)), ArrayKey::Int(-4));

        let mut array = PhpArray::new();
        array.insert(ArrayKey::Int(-1), Value::Int(1));
        assert_eq!(array.append(Value::Int(2)), ArrayKey::Int(0));

        array.insert(ArrayKey::Int(-10), Value::Int(3));
        assert_eq!(array.append(Value::Int(4)), ArrayKey::Int(1));
    }

    #[test]
    fn array_remove_and_get_mut_do_not_expose_storage() {
        let mut array = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2)]);
        *array.get_mut(&ArrayKey::Int(1)).expect("entry") = Value::Int(5);

        assert_eq!(array.get(&ArrayKey::Int(1)), Some(&Value::Int(5)));
        assert_eq!(array.remove(&ArrayKey::Int(0)), Some(Value::Int(1)));
        assert_eq!(array.len(), 1);
        assert_eq!(array.get(&ArrayKey::Int(0)), None);
    }

    #[test]
    fn foreach_snapshot_keys_keep_insertion_order_after_mutation() {
        let mut array = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2)]);
        let keys = array.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>();

        array.remove(&ArrayKey::Int(0));
        array.append(Value::Int(3));

        assert_eq!(keys, vec![ArrayKey::Int(0), ArrayKey::Int(1)]);
        assert_eq!(
            array.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>(),
            vec![ArrayKey::Int(1), ArrayKey::Int(2)]
        );
    }

    #[test]
    fn foreach_dynamic_key_reads_include_appended_entries() {
        let mut array = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2)]);
        let first_keys = array.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>();
        assert_eq!(first_keys, vec![ArrayKey::Int(0), ArrayKey::Int(1)]);

        array.append(Value::Int(3));
        let second_keys = array.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>();
        assert_eq!(
            second_keys,
            vec![ArrayKey::Int(0), ArrayKey::Int(1), ArrayKey::Int(2)]
        );
    }

    #[test]
    fn cow_array_assignment_shares_until_write() {
        let original = PhpArray::from_packed(vec![Value::Int(1)]);
        let mut copy = original.clone();

        assert!(original.is_shared());
        assert!(copy.is_shared());

        copy.append(Value::Int(2));

        assert_eq!(
            original.packed_elements().expect("packed original").len(),
            1
        );
        assert_eq!(copy.packed_elements().expect("packed copy").len(), 2);
        assert_eq!(original.get(&ArrayKey::Int(1)), None);
        assert_eq!(copy.get(&ArrayKey::Int(1)), Some(&Value::Int(2)));
        assert!(!copy.is_shared());
    }

    #[test]
    fn array_key_conversion_covers_mvp_value_types() {
        assert_eq!(
            ArrayKey::from_value_mvp(&Value::Int(4)),
            Some(ArrayKey::Int(4))
        );
        assert_eq!(
            ArrayKey::from_value_mvp(&Value::Bool(true)),
            Some(ArrayKey::Int(1))
        );
        assert_eq!(
            ArrayKey::from_value_mvp(&Value::Null),
            Some(ArrayKey::String(PhpString::from("")))
        );
        assert_eq!(
            ArrayKey::from_value_mvp(&Value::float(4.9)),
            Some(ArrayKey::Int(4))
        );
        assert_eq!(
            ArrayKey::from_value_mvp(&Value::String(PhpString::from("42"))),
            Some(ArrayKey::Int(42))
        );
        assert_eq!(
            ArrayKey::from_value_mvp(&Value::String(PhpString::from("042"))),
            Some(ArrayKey::String(PhpString::from("042")))
        );
        assert_eq!(
            ArrayKey::from_value_mvp(&Value::String(PhpString::from("+42"))),
            Some(ArrayKey::String(PhpString::from("+42")))
        );
        assert_eq!(
            ArrayKey::from_value_mvp(&Value::String(PhpString::from("-42"))),
            Some(ArrayKey::Int(-42))
        );
        assert_eq!(
            ArrayKey::from_value_mvp(&Value::String(PhpString::from("-0"))),
            Some(ArrayKey::String(PhpString::from("-0")))
        );
        assert_eq!(
            ArrayKey::from_value_mvp(&Value::String(PhpString::from("9223372036854775808"))),
            Some(ArrayKey::String(PhpString::from("9223372036854775808")))
        );
        assert_eq!(
            ArrayKey::from_php_string(PhpString::from(" 42")),
            ArrayKey::String(PhpString::from(" 42"))
        );
        assert_eq!(
            ArrayKey::from_php_string(PhpString::from("1.0")),
            ArrayKey::String(PhpString::from("1.0"))
        );
    }

    #[test]
    fn array_packed_facade_detects_contiguous_integer_keys() {
        let packed = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2)]);
        assert!(packed.is_packed_fast());
        assert_eq!(packed.packed_len_fast(), Some(2));
        assert_eq!(packed.packed_element_fast(1), Some(&Value::Int(2)));
        assert_eq!(packed.packed_element_fast(2), None);
        assert_eq!(
            packed
                .packed_elements()
                .expect("packed")
                .into_iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec![Value::Int(1), Value::Int(2)]
        );
        assert_eq!(packed.packed_element(1), Some(&Value::Int(2)));
        assert_eq!(packed.packed_element(2), None);

        let mut mixed = packed;
        mixed.remove(&ArrayKey::Int(0));
        assert!(!mixed.is_packed_fast());
        assert!(mixed.packed_elements().is_none());
        assert_eq!(mixed.packed_element(0), None);
        assert_eq!(mixed.packed_element_fast(0), None);
    }

    #[test]
    fn packed_metadata_stays_fast_for_sequential_append_and_overwrite() {
        let mut array = PhpArray::new();
        array.append(Value::Int(1));
        array.append(Value::Int(2));
        array.insert(ArrayKey::Int(1), Value::Int(5));

        assert!(array.is_packed_fast());
        assert_eq!(array.packed_len_fast(), Some(2));
        assert_eq!(array.packed_element_fast(1), Some(&Value::Int(5)));
    }

    #[test]
    fn packed_metadata_transitions_for_non_sequential_int_key() {
        let mut array = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2)]);
        array.insert(ArrayKey::Int(4), Value::Int(5));

        assert!(!array.is_packed_fast());
        assert!(array.packed_elements().is_none());
        assert_eq!(array.packed_element_fast(1), None);
    }

    #[test]
    fn packed_metadata_transitions_for_string_key() {
        let mut array = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2)]);
        array.insert(ArrayKey::String(PhpString::from("x")), Value::Int(5));

        assert!(!array.is_packed_fast());
        assert!(array.packed_elements().is_none());
    }

    #[test]
    fn packed_metadata_tracks_unset_holes_and_append_after_last_unset() {
        let mut hole = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        hole.remove(&ArrayKey::Int(1));
        assert!(!hole.is_packed_fast());
        assert!(hole.packed_elements().is_none());

        let mut tail = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        tail.remove(&ArrayKey::Int(2));
        assert!(tail.is_packed_fast());
        assert_eq!(tail.packed_len_fast(), Some(2));
        assert_eq!(tail.append(Value::Int(4)), ArrayKey::Int(3));
        assert!(!tail.is_packed_fast());
        assert!(tail.packed_elements().is_none());
    }

    #[test]
    fn packed_metadata_allows_reference_elements_without_cow_shortcuts() {
        let mut array = PhpArray::from_packed(vec![Value::Int(1)]);
        let cell = crate::ReferenceCell::new(Value::Int(2));
        array.append(Value::Reference(cell.clone()));

        assert!(array.is_packed_fast());
        assert_eq!(array.packed_len_fast(), Some(2));
        cell.set(Value::Int(7));
        assert_eq!(array.packed_element_fast(1), Some(&Value::Reference(cell)));
    }

    #[test]
    fn packed_metadata_reports_kind_references_sharing_and_epoch() {
        let mut array = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2)]);
        let metadata = array.packed_metadata();
        assert_eq!(metadata.kind, PhpArrayKind::PackedList);
        assert_eq!(metadata.element_summary, PhpArrayElementSummary::AllInt);
        assert_eq!(
            metadata.key_kind_summary,
            PhpArrayKeyKindSummary::SequentialInt
        );
        assert_eq!(metadata.packed_len, Some(2));
        assert!(!metadata.numeric_string_key_ambiguity);
        assert!(!metadata.is_shared);
        assert!(!metadata.contains_references);
        assert_eq!(metadata.mutation_epoch, 2);

        let shared = array.clone();
        assert!(array.packed_metadata().is_shared);
        drop(shared);

        let cell = crate::ReferenceCell::new(Value::Int(3));
        array.append(Value::Reference(cell));
        let metadata = array.packed_metadata();
        assert_eq!(metadata.kind, PhpArrayKind::PackedList);
        assert_eq!(metadata.element_summary, PhpArrayElementSummary::Mixed);
        assert!(metadata.contains_references);
        assert_eq!(metadata.packed_len, Some(3));
        assert_eq!(metadata.mutation_epoch, 3);

        array.insert(ArrayKey::String(PhpString::from("x")), Value::Int(4));
        let metadata = array.packed_metadata();
        assert_eq!(metadata.kind, PhpArrayKind::MixedHash);
        assert_eq!(metadata.key_kind_summary, PhpArrayKeyKindSummary::Mixed);
        assert_eq!(metadata.packed_len, None);
        assert_eq!(metadata.mutation_epoch, 4);
    }

    #[test]
    fn array_metadata_reports_key_kinds_and_numeric_string_ambiguity() {
        let empty = PhpArray::new();
        let metadata = empty.packed_metadata();
        assert_eq!(metadata.element_summary, PhpArrayElementSummary::Empty);
        assert_eq!(metadata.key_kind_summary, PhpArrayKeyKindSummary::Empty);

        let mut int_only = PhpArray::new();
        int_only.insert(ArrayKey::Int(2), Value::Int(1));
        int_only.insert(ArrayKey::Int(4), Value::Int(2));
        assert_eq!(
            int_only.packed_metadata().key_kind_summary,
            PhpArrayKeyKindSummary::IntOnly
        );

        let mut string_only = PhpArray::new();
        string_only.insert(ArrayKey::String(PhpString::from("01")), Value::Int(1));
        string_only.insert(ArrayKey::String(PhpString::from("name")), Value::Int(2));
        let metadata = string_only.packed_metadata();
        assert_eq!(
            metadata.key_kind_summary,
            PhpArrayKeyKindSummary::StringOnly
        );
        assert!(metadata.numeric_string_key_ambiguity);
    }

    #[test]
    fn packed_int_sum_fast_is_guarded_by_layout_aliasing_type_and_overflow() {
        assert_eq!(
            PhpArray::from_packed(vec![Value::Int(4), Value::Int(8)]).packed_int_sum_fast(),
            Ok(12)
        );
        assert_eq!(PhpArray::new().packed_int_sum_fast(), Ok(0));

        let mut mixed_layout = PhpArray::from_packed(vec![Value::Int(1)]);
        mixed_layout.insert(ArrayKey::String(PhpString::from("x")), Value::Int(2));
        assert_eq!(
            mixed_layout.packed_int_sum_fast(),
            Err(PhpArrayPackedIntReductionError::NotPacked)
        );

        let shared = PhpArray::from_packed(vec![Value::Int(1)]);
        let shared_copy = shared.clone();
        assert_eq!(
            shared.packed_int_sum_fast(),
            Err(PhpArrayPackedIntReductionError::Shared)
        );
        drop(shared_copy);

        let reference = crate::ReferenceCell::new(Value::Int(1));
        assert_eq!(
            PhpArray::from_packed(vec![Value::Reference(reference)]).packed_int_sum_fast(),
            Err(PhpArrayPackedIntReductionError::ContainsReferences)
        );
        assert_eq!(
            PhpArray::from_packed(vec![Value::Int(1), Value::string("2")]).packed_int_sum_fast(),
            Err(PhpArrayPackedIntReductionError::NonIntElement)
        );
        assert_eq!(
            PhpArray::from_packed(vec![Value::Int(i64::MAX), Value::Int(1)]).packed_int_sum_fast(),
            Err(PhpArrayPackedIntReductionError::Overflow)
        );
    }

    #[test]
    fn mutation_epoch_tracks_value_and_structural_writes() {
        let mut array = PhpArray::new();
        assert_eq!(array.mutation_epoch(), 0);

        array.append(Value::Int(1));
        assert_eq!(array.mutation_epoch(), 1);

        array.insert(ArrayKey::Int(0), Value::Int(2));
        assert_eq!(array.mutation_epoch(), 2);

        *array.get_mut(&ArrayKey::Int(0)).expect("entry") = Value::Int(3);
        assert_eq!(array.mutation_epoch(), 3);

        assert_eq!(array.remove(&ArrayKey::Int(0)), Some(Value::Int(3)));
        assert_eq!(array.mutation_epoch(), 4);
        assert_eq!(array.remove(&ArrayKey::Int(99)), None);
        assert_eq!(array.mutation_epoch(), 4);
    }

    #[test]
    fn mutation_epoch_is_not_php_visible_equality() {
        let mut first = PhpArray::from_packed(vec![Value::Int(1)]);
        let second = PhpArray::from_packed(vec![Value::Int(1)]);
        first.insert(ArrayKey::Int(0), Value::Int(1));

        assert_ne!(first.mutation_epoch(), second.mutation_epoch());
        assert_eq!(first, second);
    }

    #[test]
    fn array_shape_metadata_classifies_prompt_shapes() {
        let empty = PhpArray::new();
        assert_eq!(empty.shape_metadata().kind, PhpArrayShapeKind::Empty);

        let packed = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2)]);
        assert_eq!(packed.shape_metadata().kind, PhpArrayShapeKind::Packed);

        let mut holes = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2)]);
        holes.remove(&ArrayKey::Int(0));
        assert_eq!(
            holes.shape_metadata().kind,
            PhpArrayShapeKind::PackedWithHoles
        );

        let mut small = PhpArray::new();
        small.insert(ArrayKey::Int(1), Value::Int(1));
        small.insert(ArrayKey::String(PhpString::from("name")), Value::Int(2));
        assert_eq!(
            small.shape_metadata().kind,
            PhpArrayShapeKind::SmallInlineMap
        );

        let mut record = PhpArray::new();
        record.insert(ArrayKey::String(PhpString::from("id")), Value::Int(1));
        record.insert(ArrayKey::String(PhpString::from("name")), Value::Int(2));
        assert_eq!(
            record.shape_metadata().kind,
            PhpArrayShapeKind::ShapeStableRecordLike
        );

        let shared_key = PhpString::from("id");
        let mut interned_record = PhpArray::new();
        interned_record.insert(ArrayKey::String(shared_key.clone()), Value::Int(1));
        assert!(shared_key.is_shared());
        assert_eq!(
            interned_record.shape_metadata().kind,
            PhpArrayShapeKind::InternedStringKeyRecord
        );

        let mut shared_candidate = PhpArray::new();
        shared_candidate.insert(ArrayKey::Int(1), Value::Int(1));
        shared_candidate.insert(ArrayKey::String(PhpString::from("name")), Value::Int(2));
        shared_candidate.insert(ArrayKey::Int(3), Value::Int(3));
        shared_candidate.insert(ArrayKey::String(PhpString::from("slug")), Value::Int(4));
        shared_candidate.insert(ArrayKey::Int(5), Value::Int(5));
        let shared = shared_candidate.clone();
        assert_eq!(
            shared.shape_metadata().kind,
            PhpArrayShapeKind::SharedImmutableLiteralArray
        );

        let cell = crate::ReferenceCell::new(Value::Int(1));
        let mut reference_array = PhpArray::new();
        reference_array.insert(
            ArrayKey::String(PhpString::from("id")),
            Value::Reference(cell),
        );
        assert_eq!(
            reference_array.shape_metadata().kind,
            PhpArrayShapeKind::CowOrReferenceFallback
        );
    }

    #[test]
    fn record_and_small_map_shape_lookups_fail_closed() {
        let mut record = PhpArray::new();
        record.insert(ArrayKey::String(PhpString::from("id")), Value::Int(1));

        assert!(matches!(
            record.record_shape_string_key_lookup(&ArrayKey::String(PhpString::from("id"))),
            PhpArrayShapeLookup::Hit(Value::Int(1))
        ));
        assert_eq!(
            record.record_shape_string_key_lookup(&ArrayKey::String(PhpString::from("missing"))),
            PhpArrayShapeLookup::Miss
        );
        assert_eq!(
            record.record_shape_string_key_lookup(&ArrayKey::Int(0)),
            PhpArrayShapeLookup::Fallback(PhpArrayShapeLookupFallback::KeyCoercion)
        );

        let mut numeric_string = PhpArray::new();
        numeric_string.insert(ArrayKey::String(PhpString::from("01")), Value::Int(1));
        assert_eq!(
            numeric_string.record_shape_string_key_lookup(&ArrayKey::String(PhpString::from("01"))),
            PhpArrayShapeLookup::Fallback(PhpArrayShapeLookupFallback::KeyCoercion)
        );

        let mut small = PhpArray::new();
        small.insert(ArrayKey::Int(1), Value::Int(1));
        small.insert(ArrayKey::String(PhpString::from("name")), Value::Int(2));
        assert!(matches!(
            small.small_map_lookup(&ArrayKey::String(PhpString::from("name"))),
            PhpArrayShapeLookup::Hit(Value::Int(2))
        ));

        let shared = small.clone();
        assert!(matches!(
            shared.small_map_lookup(&ArrayKey::String(PhpString::from("name"))),
            PhpArrayShapeLookup::Hit(Value::Int(2))
        ));

        let cell = crate::ReferenceCell::new(Value::Int(1));
        let mut reference_array = PhpArray::new();
        reference_array.insert(
            ArrayKey::String(PhpString::from("id")),
            Value::Reference(cell),
        );
        assert_eq!(
            reference_array
                .record_shape_string_key_lookup(&ArrayKey::String(PhpString::from("id"))),
            PhpArrayShapeLookup::Fallback(PhpArrayShapeLookupFallback::CowOrReference)
        );
    }
}
