//! Opaque ordered PHP array storage for runtime-semantics.

use crate::layout_stats::{
    PackedToMixedReason, RecordToMixedReason, SOURCE_FOREACH_VALUE,
    enter_default_layout_source_family,
};
use crate::{
    PhpString, Value,
    numeric_string::{
        NumericStringArrayKey, array_key_has_numeric_string_ambiguity, classify_array_key,
    },
};
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    rc::{Rc, Weak},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_ARRAY_STORAGE_ID: AtomicU64 = AtomicU64::new(1);

fn next_array_storage_id() -> u64 {
    NEXT_ARRAY_STORAGE_ID.fetch_add(1, Ordering::Relaxed)
}

/// PHP array key after runtime-semantics key normalization.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ArrayKey {
    /// Integer array key.
    Int(i64),
    /// String array key.
    String(PhpString),
}

/// PHP-visible failure when an implicit array append has no free integer key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhpArrayAppendError;

/// Canonical PHP message for an exhausted implicit array key.
pub const PHP_ARRAY_APPEND_OVERFLOW_MESSAGE: &str =
    "Cannot add element to the array as the next element is already occupied";

impl std::fmt::Display for PhpArrayAppendError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(PHP_ARRAY_APPEND_OVERFLOW_MESSAGE)
    }
}

impl std::error::Error for PhpArrayAppendError {}

impl ArrayKey {
    /// Converts a runtime value into a runtime-semantics PHP array key.
    ///
    /// Supported conversions:
    /// - `int` remains an integer key;
    /// - `bool` becomes `0` or `1`;
    /// - `null` becomes the empty-string key;
    /// - `float` truncates toward zero;
    /// - `resource` becomes its numeric resource ID when representable;
    /// - decimal integer strings without a leading plus and without leading
    ///   zeroes become integer keys;
    /// - all other strings remain string keys.
    #[must_use]
    pub fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Int(value) => Some(Self::Int(*value)),
            Value::Bool(false) => Some(Self::Int(0)),
            Value::Bool(true) => Some(Self::Int(1)),
            Value::Null => Some(Self::String(PhpString::from_bytes(Vec::new()))),
            Value::Float(value) => {
                Some(Self::Int(crate::convert::php_float_to_int(value.to_f64())))
            }
            Value::String(value) => Some(Self::from_php_string(value.clone())),
            Value::Uninitialized => Some(Self::String(PhpString::from_bytes(Vec::new()))),
            Value::Resource(resource) => i64::try_from(resource.id().get()).ok().map(Self::Int),
            Value::Array(_)
            | Value::Object(_)
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

/// PHP-visible reason an array is being prepared for mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhpArrayWriteIntent {
    VariableWrite,
    NestedDimensionWrite,
    Append,
    BindReferenceElement,
    Remove,
    PointerMutation,
}

impl PhpArrayWriteIntent {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VariableWrite => "variable_write",
            Self::NestedDimensionWrite => "nested_dimension_write",
            Self::Append => "append",
            Self::BindReferenceElement => "bind_reference_element",
            Self::Remove => "remove",
            Self::PointerMutation => "pointer_mutation",
        }
    }
}

/// One ordered array slot.
#[derive(Clone, Debug)]
pub struct ArrayEntry {
    key: ArrayKey,
    value: Value,
    string_key_shared_for_shape: bool,
}

impl ArrayEntry {
    /// Consumes the entry into its key and value.
    #[must_use]
    pub fn into_key_value(self) -> (ArrayKey, Value) {
        (self.key, self.value)
    }
}

impl PartialEq for ArrayEntry {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.value == other.value
    }
}

impl Eq for ArrayEntry {}

impl ArrayEntry {
    fn new(key: ArrayKey, value: Value) -> Self {
        let string_key_shared_for_shape = matches!(&key, ArrayKey::String(key) if key.is_shared());
        Self {
            key,
            value,
            string_key_shared_for_shape,
        }
    }

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

/// Non-allocating borrowed iterator over proven packed-array values.
pub struct PackedArrayValues<'a> {
    values: std::slice::Iter<'a, Value>,
}

impl<'a> Iterator for PackedArrayValues<'a> {
    type Item = &'a Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.values.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.values.size_hint()
    }
}

impl ExactSizeIterator for PackedArrayValues<'_> {}

/// Insertion-order iterator over array pairs. Packed arrays synthesize
/// their sequential integer keys instead of storing them.
pub enum PhpArrayIter<'a> {
    Packed(std::iter::Enumerate<std::slice::Iter<'a, Value>>),
    Record(std::iter::Zip<std::slice::Iter<'a, PhpString>, std::slice::Iter<'a, Value>>),
    Mixed(MixedArrayIter<'a>),
}

pub struct MixedArrayIter<'a> {
    entries: std::slice::Iter<'a, Option<ArrayEntry>>,
    remaining: usize,
}

impl<'a> Iterator for PhpArrayIter<'a> {
    type Item = (ArrayKey, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Packed(values) => values
                .next()
                .map(|(index, value)| (ArrayKey::Int(index as i64), value)),
            Self::Record(pairs) => pairs
                .next()
                .map(|(key, value)| (ArrayKey::String(key.clone()), value)),
            Self::Mixed(entries) => {
                for entry in entries.entries.by_ref() {
                    let Some(entry) = entry.as_ref() else {
                        continue;
                    };
                    entries.remaining -= 1;
                    return Some((entry.key.clone(), &entry.value));
                }
                None
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Packed(values) => values.size_hint(),
            Self::Record(pairs) => pairs.size_hint(),
            Self::Mixed(entries) => (entries.remaining, Some(entries.remaining)),
        }
    }
}

impl ExactSizeIterator for PhpArrayIter<'_> {}

/// Mutable array slot guard that refreshes value-dependent metadata when the
/// caller finishes mutating the value.
pub struct PhpArrayValueMut<'a> {
    storage: &'a mut ArrayStorage,
    index: usize,
    old_is_reference: bool,
    old_is_int: bool,
}

impl Deref for PhpArrayValueMut<'_> {
    type Target = Value;

    fn deref(&self) -> &Self::Target {
        self.storage.value_at(self.index)
    }
}

impl DerefMut for PhpArrayValueMut<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.storage.value_at_mut(self.index)
    }
}

impl Drop for PhpArrayValueMut<'_> {
    fn drop(&mut self) {
        let value = self.storage.value_at(self.index);
        let new_is_reference = matches!(value, Value::Reference(_));
        let new_is_int = matches!(value, Value::Int(_));
        self.storage.metadata_mut().replace_value_flags(
            self.old_is_reference,
            new_is_reference,
            self.old_is_int,
            new_is_int,
        );
        self.storage.debug_assert_consistent();
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ArrayCachedMetadata {
    len: usize,
    reference_values: usize,
    int_values: usize,
    int_keys: usize,
    string_keys: usize,
    numeric_string_ambiguous_keys: usize,
    shared_string_keys: usize,
}

impl ArrayCachedMetadata {
    fn from_entry_iter_for_debug<'a>(entries: impl IntoIterator<Item = &'a ArrayEntry>) -> Self {
        let mut metadata = Self::default();
        for entry in entries {
            metadata.add_entry(entry);
        }
        metadata
    }

    fn from_packed_values(values: &[Value]) -> Self {
        crate::layout_stats::record_array_metadata_recompute();
        Self::from_packed_values_without_counter(values)
    }

    fn from_packed_values_without_counter(values: &[Value]) -> Self {
        Self::from_record_values_without_counter(values, None)
    }

    /// Values-only metadata: integer keys when `string_key_count` is `None`,
    /// otherwise that many interner-shared string keys.
    fn from_record_values_without_counter(
        values: &[Value],
        string_key_count: Option<usize>,
    ) -> Self {
        let mut metadata = Self::default();
        for value in values {
            match string_key_count {
                Some(_) => metadata.add_record_value(value),
                None => metadata.add_packed_value(value),
            }
        }
        metadata
    }

    /// Packed slots always carry the next sequential integer key.
    fn add_packed_value(&mut self, value: &Value) {
        self.len += 1;
        if matches!(value, Value::Reference(_)) {
            self.reference_values += 1;
        }
        if matches!(value, Value::Int(_)) {
            self.int_values += 1;
        }
        self.int_keys += 1;
    }

    /// Record slots carry interned (always-shared) string keys with no
    /// numeric-string ambiguity, enforced at promotion.
    fn add_record_value(&mut self, value: &Value) {
        self.len += 1;
        if matches!(value, Value::Reference(_)) {
            self.reference_values += 1;
        }
        if matches!(value, Value::Int(_)) {
            self.int_values += 1;
        }
        self.string_keys += 1;
        self.shared_string_keys += 1;
    }

    fn remove_packed_value(&mut self, value: &Value) {
        self.len -= 1;
        if matches!(value, Value::Reference(_)) {
            self.reference_values -= 1;
        }
        if matches!(value, Value::Int(_)) {
            self.int_values -= 1;
        }
        self.int_keys -= 1;
    }

    fn add_entry(&mut self, entry: &ArrayEntry) {
        self.len += 1;
        if matches!(entry.value, Value::Reference(_)) {
            self.reference_values += 1;
        }
        if matches!(entry.value, Value::Int(_)) {
            self.int_values += 1;
        }
        match &entry.key {
            ArrayKey::Int(_) => self.int_keys += 1,
            ArrayKey::String(key) => {
                self.string_keys += 1;
                if array_key_has_numeric_string_ambiguity(key) {
                    self.numeric_string_ambiguous_keys += 1;
                }
                if entry.string_key_shared_for_shape {
                    self.shared_string_keys += 1;
                }
            }
        }
    }

    fn remove_entry(&mut self, entry: &ArrayEntry) {
        self.len -= 1;
        if matches!(entry.value, Value::Reference(_)) {
            self.reference_values -= 1;
        }
        if matches!(entry.value, Value::Int(_)) {
            self.int_values -= 1;
        }
        match &entry.key {
            ArrayKey::Int(_) => self.int_keys -= 1,
            ArrayKey::String(key) => {
                self.string_keys -= 1;
                if array_key_has_numeric_string_ambiguity(key) {
                    self.numeric_string_ambiguous_keys -= 1;
                }
                if entry.string_key_shared_for_shape {
                    self.shared_string_keys -= 1;
                }
            }
        }
    }

    fn replace_value_flags(
        &mut self,
        old_is_reference: bool,
        new_is_reference: bool,
        old_is_int: bool,
        new_is_int: bool,
    ) {
        if old_is_reference {
            self.reference_values -= 1;
        }
        if new_is_reference {
            self.reference_values += 1;
        }
        if old_is_int {
            self.int_values -= 1;
        }
        if new_is_int {
            self.int_values += 1;
        }
    }

    const fn contains_references(self) -> bool {
        self.reference_values > 0
    }

    const fn element_summary(self) -> PhpArrayElementSummary {
        if self.len == 0 {
            PhpArrayElementSummary::Empty
        } else if self.int_values == self.len {
            PhpArrayElementSummary::AllInt
        } else {
            PhpArrayElementSummary::Mixed
        }
    }

    const fn key_kind_summary(self, is_packed: bool) -> PhpArrayKeyKindSummary {
        if self.len == 0 {
            PhpArrayKeyKindSummary::Empty
        } else if is_packed {
            PhpArrayKeyKindSummary::SequentialInt
        } else if self.int_keys == self.len {
            PhpArrayKeyKindSummary::IntOnly
        } else if self.string_keys == self.len {
            PhpArrayKeyKindSummary::StringOnly
        } else {
            PhpArrayKeyKindSummary::Mixed
        }
    }

    const fn has_numeric_string_key_ambiguity(self) -> bool {
        self.numeric_string_ambiguous_keys > 0
    }

    const fn string_keys_share_storage(self) -> bool {
        self.string_keys == self.shared_string_keys
    }
}

/// Packed array storage: values only, keys are virtually `0..len`.
///
/// Iteration synthesizes the sequential integer keys, so a packed slot is
/// one `Value` instead of a full key/value entry.
#[derive(Clone, Debug)]
struct PackedArrayStorage {
    storage_id: u64,
    values: Vec<Value>,
    next_append_key: Option<i64>,
    internal_pointer: Option<usize>,
    mutation_epoch: u64,
    cached_metadata: ArrayCachedMetadata,
}

/// Shared key layout for record-like string-key maps. Shapes are interned
/// per thread so repeated map literals (config rows, translation tables,
/// route parameters) share one key table and slot index.
struct RecordShape {
    shape_id: u64,
    /// Interned string keys in insertion order, slot-index aligned.
    keys: Vec<PhpString>,
    /// key -> slot index; probe keys compare by symbol identity or bytes.
    slot_by_key: StableKeyMap<PhpString, u32>,
    /// Memoized `shape + key -> extended shape` transitions. Growing a
    /// string-key map appends one key per insert; without this cache every
    /// insert re-interns and re-hashes the entire key sequence, making
    /// registry-style array growth quadratic in the number of keys.
    #[allow(clippy::mutable_key_type)] // PhpString hash/eq are byte-pure.
    transitions: std::cell::RefCell<StableKeyMap<PhpString, Rc<RecordShape>>>,
}

impl std::fmt::Debug for RecordShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordShape")
            .field("shape_id", &self.shape_id)
            .field("len", &self.keys.len())
            .finish()
    }
}

thread_local! {
    static RECORD_SHAPE_CACHE: std::cell::RefCell<HashMap<u64, Vec<Rc<RecordShape>>>> =
        std::cell::RefCell::new(HashMap::new());
    static NEXT_RECORD_SHAPE_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
}

fn record_shape_sequence_hash(keys: &[PhpString], appended: Option<&PhpString>) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for key in keys.iter().chain(appended) {
        hash ^= key.stable_hash();
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Interns the shape for `keys + appended`, reusing an existing shape with
/// the same key sequence. Interned shapes are held strongly (they are never
/// extended in place; only private, unregistered shapes qualify for
/// `Rc::get_mut`), so rebuilt-per-request maps reuse their chains instead of
/// re-interning every key sequence.
fn record_shape_for(keys: &[PhpString], appended: Option<&PhpString>) -> Rc<RecordShape> {
    let sequence_hash = record_shape_sequence_hash(keys, appended);
    RECORD_SHAPE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let candidates = cache.entry(sequence_hash).or_default();
        if let Some(existing) = candidates.iter().find(|shape| {
            shape.keys.len() == keys.len() + usize::from(appended.is_some())
                && shape
                    .keys
                    .iter()
                    .zip(keys.iter().chain(appended))
                    .all(|(a, b)| a.same_symbol_or_bytes(b))
        }) {
            return Rc::clone(existing);
        }
        let interned_keys = keys
            .iter()
            .chain(appended)
            .map(|key| PhpString::intern(key.as_bytes()))
            .collect::<Vec<_>>();
        #[allow(clippy::mutable_key_type)]
        let slot_by_key = interned_keys
            .iter()
            .enumerate()
            .map(|(index, key)| (key.clone(), index as u32))
            .collect::<StableKeyMap<_, _>>();
        let shape_id = NEXT_RECORD_SHAPE_ID.with(|next| {
            let id = next.get();
            next.set(id.wrapping_add(1));
            id
        });
        let shape = Rc::new(RecordShape {
            shape_id,
            keys: interned_keys,
            slot_by_key,
            transitions: std::cell::RefCell::new(StableKeyMap::default()),
        });
        candidates.push(Rc::clone(&shape));
        shape
    })
}

/// Key count past which a growing record array stops interning every
/// intermediate shape and switches to a private, in-place-extendable shape.
/// Small maps (config rows, translation entries) keep interned shared
/// shapes; registry-style maps that keep growing would otherwise pay an
/// O(len) shape rebuild per insert.
const RECORD_SHAPE_PRIVATE_GROWTH_THRESHOLD: usize = 32;

/// Builds `shape + key` as a private, cache-unregistered shape. The result
/// has no weak registrations, so the owning array's next append can extend
/// it in place through `Rc::get_mut` at O(1).
fn record_shape_private_extended(shape: &RecordShape, key: &PhpString) -> Rc<RecordShape> {
    let interned = PhpString::intern(key.as_bytes());
    let mut keys = Vec::with_capacity(shape.keys.len() + 1);
    keys.extend(shape.keys.iter().cloned());
    #[allow(clippy::mutable_key_type)] // PhpString hash/eq are byte-pure.
    let mut slot_by_key = shape.slot_by_key.clone();
    slot_by_key.insert(interned.clone(), keys.len() as u32);
    keys.push(interned);
    let shape_id = NEXT_RECORD_SHAPE_ID.with(|next| {
        let id = next.get();
        next.set(id.wrapping_add(1));
        id
    });
    Rc::new(RecordShape {
        shape_id,
        keys,
        slot_by_key,
        transitions: std::cell::RefCell::new(StableKeyMap::default()),
    })
}

/// Extends `shape` by one appended key, memoizing the transition on the
/// parent shape so repeated growth of the same key sequence is O(1) per
/// insert instead of re-interning the whole sequence.
///
/// Interned shapes are immutable (in-place extension is reserved for
/// private, unregistered shapes), so memoized targets always represent
/// `parent + key`; the length/last-key check stays as a cheap guard.
fn record_shape_extended(shape: &Rc<RecordShape>, key: &PhpString) -> Rc<RecordShape> {
    if let Some(next) = shape.transitions.borrow().get(key)
        && next.keys.len() == shape.keys.len() + 1
        && next
            .keys
            .last()
            .is_some_and(|last| last.same_symbol_or_bytes(key))
    {
        return Rc::clone(next);
    }
    let next = record_shape_for(&shape.keys, Some(key));
    shape
        .transitions
        .borrow_mut()
        .insert(next.keys[shape.keys.len()].clone(), Rc::clone(&next));
    next
}

/// Record storage: stable string-key maps with a shared key shape and a
/// values-only slot vector.
#[derive(Clone, Debug)]
struct RecordArrayStorage {
    storage_id: u64,
    shape: Rc<RecordShape>,
    values: Vec<Value>,
    next_append_key: Option<i64>,
    internal_pointer: Option<usize>,
    mutation_epoch: u64,
    cached_metadata: ArrayCachedMetadata,
}

/// Mixed array storage for holes, string keys, and non-sequential integer keys.
#[derive(Clone, Debug)]
struct MixedArrayStorage {
    storage_id: u64,
    entries: Vec<Option<ArrayEntry>>,
    live_len: usize,
    index: StableKeyMap<ArrayKey, usize>,
    next_append_key: Option<i64>,
    internal_pointer: Option<usize>,
    mutation_epoch: u64,
    cached_metadata: ArrayCachedMetadata,
}

/// Ordered PHP array storage.
///
/// The storage is intentionally opaque. Callers interact through key/value APIs
/// and shape metadata, not through packed or mixed internals.
#[derive(Clone, Debug)]
enum ArrayStorage {
    Packed(PackedArrayStorage),
    Record(RecordArrayStorage),
    Mixed(MixedArrayStorage),
}

impl Default for ArrayStorage {
    fn default() -> Self {
        Self::Packed(PackedArrayStorage {
            storage_id: next_array_storage_id(),
            values: Vec::new(),
            next_append_key: None,
            internal_pointer: None,
            mutation_epoch: 0,
            cached_metadata: ArrayCachedMetadata::default(),
        })
    }
}

impl PartialEq for ArrayStorage {
    fn eq(&self, other: &Self) -> bool {
        if self.next_append_key() != other.next_append_key()
            || self.internal_pointer() != other.internal_pointer()
            || self.len() != other.len()
        {
            return false;
        }
        match (self, other) {
            (Self::Packed(lhs), Self::Packed(rhs)) => lhs.values == rhs.values,
            (Self::Mixed(lhs), Self::Mixed(rhs)) => lhs
                .entries
                .iter()
                .filter_map(Option::as_ref)
                .eq(rhs.entries.iter().filter_map(Option::as_ref)),
            (Self::Record(lhs), Self::Record(rhs)) => {
                (Rc::ptr_eq(&lhs.shape, &rhs.shape)
                    || lhs
                        .shape
                        .keys
                        .iter()
                        .zip(&rhs.shape.keys)
                        .all(|(a, b)| a.same_symbol_or_bytes(b)))
                    && lhs.values == rhs.values
            }
            (Self::Packed(packed), Self::Mixed(mixed))
            | (Self::Mixed(mixed), Self::Packed(packed)) => {
                mixed.entries.iter().filter_map(Option::as_ref).enumerate().all(|(index, entry)| {
                    matches!(&entry.key, ArrayKey::Int(key) if *key == index as i64)
                        && entry.value == packed.values[index]
                })
            }
            (Self::Record(record), Self::Mixed(mixed))
            | (Self::Mixed(mixed), Self::Record(record)) => {
                mixed.entries.iter().filter_map(Option::as_ref).enumerate().all(|(index, entry)| {
                    matches!(&entry.key, ArrayKey::String(key) if key.same_symbol_or_bytes(&record.shape.keys[index]))
                        && entry.value == record.values[index]
                })
            }
            (Self::Packed(packed), Self::Record(record))
            | (Self::Record(record), Self::Packed(packed)) => {
                packed.values.is_empty() && record.values.is_empty()
            }
        }
    }
}

impl Eq for ArrayStorage {}

impl ArrayStorage {
    fn storage_id(&self) -> u64 {
        match self {
            Self::Packed(storage) => storage.storage_id,
            Self::Record(storage) => storage.storage_id,
            Self::Mixed(storage) => storage.storage_id,
        }
    }

    fn set_storage_id(&mut self, storage_id: u64) {
        match self {
            Self::Packed(storage) => storage.storage_id = storage_id,
            Self::Record(storage) => storage.storage_id = storage_id,
            Self::Mixed(storage) => storage.storage_id = storage_id,
        }
    }
    const MIXED_COMPACTION_MIN_TOMBSTONES: usize = 32;

    fn value_at(&self, index: usize) -> &Value {
        match self {
            Self::Packed(storage) => &storage.values[index],
            Self::Record(storage) => &storage.values[index],
            Self::Mixed(storage) => {
                &storage.entries[index]
                    .as_ref()
                    .expect("mixed index must point to a live slot")
                    .value
            }
        }
    }

    fn value_at_mut(&mut self, index: usize) -> &mut Value {
        match self {
            Self::Packed(storage) => &mut storage.values[index],
            Self::Record(storage) => &mut storage.values[index],
            Self::Mixed(storage) => {
                &mut storage.entries[index]
                    .as_mut()
                    .expect("mixed index must point to a live slot")
                    .value
            }
        }
    }

    fn get_value(&self, index: usize) -> Option<&Value> {
        match self {
            Self::Packed(storage) => storage.values.get(index),
            Self::Record(storage) => storage.values.get(index),
            Self::Mixed(storage) => storage
                .entries
                .get(index)
                .and_then(Option::as_ref)
                .map(ArrayEntry::value),
        }
    }

    /// Key at an index; packed and record keys come from the shape.
    fn key_at(&self, index: usize) -> Option<ArrayKey> {
        match self {
            Self::Packed(storage) => {
                (index < storage.values.len()).then_some(ArrayKey::Int(index as i64))
            }
            Self::Record(storage) => storage
                .shape
                .keys
                .get(index)
                .map(|key| ArrayKey::String(key.clone())),
            Self::Mixed(storage) => storage
                .entries
                .get(index)
                .and_then(Option::as_ref)
                .map(|entry| entry.key.clone()),
        }
    }

    fn iter_pairs(&self) -> PhpArrayIter<'_> {
        match self {
            Self::Packed(storage) => {
                crate::layout_stats::record_packed_virtual_key_iteration();
                PhpArrayIter::Packed(storage.values.iter().enumerate())
            }
            Self::Record(storage) => {
                PhpArrayIter::Record(storage.shape.keys.iter().zip(storage.values.iter()))
            }
            Self::Mixed(storage) => PhpArrayIter::Mixed(MixedArrayIter {
                entries: storage.entries.iter(),
                remaining: storage.live_len,
            }),
        }
    }

    fn metadata(&self) -> ArrayCachedMetadata {
        match self {
            Self::Packed(storage) => storage.cached_metadata,
            Self::Record(storage) => storage.cached_metadata,
            Self::Mixed(storage) => storage.cached_metadata,
        }
    }

    fn metadata_mut(&mut self) -> &mut ArrayCachedMetadata {
        match self {
            Self::Packed(storage) => &mut storage.cached_metadata,
            Self::Record(storage) => &mut storage.cached_metadata,
            Self::Mixed(storage) => &mut storage.cached_metadata,
        }
    }

    fn next_append_key(&self) -> Option<i64> {
        match self {
            Self::Packed(storage) => storage.next_append_key,
            Self::Record(storage) => storage.next_append_key,
            Self::Mixed(storage) => storage.next_append_key,
        }
    }

    fn set_next_append_key(&mut self, value: Option<i64>) {
        match self {
            Self::Packed(storage) => storage.next_append_key = value,
            Self::Record(storage) => storage.next_append_key = value,
            Self::Mixed(storage) => storage.next_append_key = value,
        }
    }

    fn internal_pointer(&self) -> Option<usize> {
        match self {
            Self::Packed(storage) => storage.internal_pointer,
            Self::Record(storage) => storage.internal_pointer,
            Self::Mixed(storage) => storage.internal_pointer,
        }
    }

    fn set_internal_pointer(&mut self, value: Option<usize>) {
        match self {
            Self::Packed(storage) => storage.internal_pointer = value,
            Self::Record(storage) => storage.internal_pointer = value,
            Self::Mixed(storage) => storage.internal_pointer = value,
        }
    }

    fn mutation_epoch(&self) -> u64 {
        match self {
            Self::Packed(storage) => storage.mutation_epoch,
            Self::Record(storage) => storage.mutation_epoch,
            Self::Mixed(storage) => storage.mutation_epoch,
        }
    }

    fn set_mutation_epoch(&mut self, value: u64) {
        match self {
            Self::Packed(storage) => storage.mutation_epoch = value,
            Self::Record(storage) => storage.mutation_epoch = value,
            Self::Mixed(storage) => storage.mutation_epoch = value,
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Packed(storage) => storage.values.len(),
            Self::Record(storage) => storage.values.len(),
            Self::Mixed(storage) => storage.live_len,
        }
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn first_index(&self) -> Option<usize> {
        match self {
            Self::Packed(storage) => (!storage.values.is_empty()).then_some(0),
            Self::Record(storage) => (!storage.values.is_empty()).then_some(0),
            Self::Mixed(storage) => storage.entries.iter().position(Option::is_some),
        }
    }

    fn last_index(&self) -> Option<usize> {
        match self {
            Self::Packed(storage) => storage.values.len().checked_sub(1),
            Self::Record(storage) => storage.values.len().checked_sub(1),
            Self::Mixed(storage) => storage.entries.iter().rposition(Option::is_some),
        }
    }

    fn next_index(&self, index: usize) -> Option<usize> {
        match self {
            Self::Packed(storage) => (index + 1 < storage.values.len()).then_some(index + 1),
            Self::Record(storage) => (index + 1 < storage.values.len()).then_some(index + 1),
            Self::Mixed(storage) => storage
                .entries
                .iter()
                .enumerate()
                .skip(index.saturating_add(1))
                .find_map(|(index, entry)| entry.is_some().then_some(index)),
        }
    }

    fn previous_index(&self, index: usize) -> Option<usize> {
        match self {
            Self::Packed(_) | Self::Record(_) => index.checked_sub(1),
            Self::Mixed(storage) => storage.entries[..index].iter().rposition(Option::is_some),
        }
    }

    fn compact_mixed_if_needed(&mut self) {
        let Self::Mixed(storage) = self else {
            return;
        };
        let tombstones = storage.entries.len().saturating_sub(storage.live_len);
        if tombstones < Self::MIXED_COMPACTION_MIN_TOMBSTONES
            || tombstones.saturating_mul(2) < storage.entries.len()
        {
            return;
        }

        let old_pointer = storage.internal_pointer;
        let mut pointer = None;
        let mut entries = Vec::with_capacity(storage.live_len);
        #[allow(clippy::mutable_key_type)] // ArrayKey hash/eq are byte-pure.
        let mut index = StableKeyMap::with_capacity_and_hasher(storage.live_len, StableKeyState);
        for (old_index, entry) in std::mem::take(&mut storage.entries).into_iter().enumerate() {
            let Some(entry) = entry else {
                continue;
            };
            let new_index = entries.len();
            if old_pointer == Some(old_index) {
                pointer = Some(new_index);
            }
            let previous = index.insert(entry.key.clone(), new_index);
            debug_assert!(previous.is_none());
            entries.push(Some(entry));
        }
        storage.entries = entries;
        storage.index = index;
        storage.internal_pointer = pointer;
        self.debug_assert_consistent();
    }

    fn is_packed(&self) -> bool {
        matches!(self, Self::Packed(_))
    }

    fn is_record(&self) -> bool {
        matches!(self, Self::Record(_))
    }

    /// Promotes an empty packed array into record storage. The caller has
    /// verified the incoming key is an unambiguous string key.
    fn promote_empty_to_record(&mut self) {
        debug_assert!(self.is_packed() && self.is_empty());
        crate::layout_stats::record_record_storage_array();
        crate::layout_stats::record_record_shape_promotion();
        let storage_id = self.storage_id();
        *self = Self::Record(RecordArrayStorage {
            storage_id,
            shape: record_shape_for(&[], None),
            values: Vec::new(),
            next_append_key: self.next_append_key(),
            internal_pointer: self.internal_pointer(),
            mutation_epoch: self.mutation_epoch(),
            cached_metadata: ArrayCachedMetadata::default(),
        });
    }

    /// Converts record storage to mixed, synthesizing full entries.
    fn make_record_mixed(&mut self, reason: RecordToMixedReason) {
        let Self::Record(storage) = self else {
            return;
        };
        crate::layout_stats::record_record_to_mixed(reason);
        let keys = storage.shape.keys.clone();
        let entries = std::mem::take(&mut storage.values)
            .into_iter()
            .zip(keys)
            .map(|(value, key)| ArrayEntry::new(ArrayKey::String(key), value))
            .collect::<Vec<_>>();
        #[allow(clippy::mutable_key_type)]
        let index = build_index(&entries);
        let mixed = MixedArrayStorage {
            storage_id: storage.storage_id,
            live_len: entries.len(),
            entries: entries.into_iter().map(Some).collect(),
            index,
            next_append_key: storage.next_append_key,
            internal_pointer: storage.internal_pointer,
            mutation_epoch: storage.mutation_epoch,
            cached_metadata: storage.cached_metadata,
        };
        *self = Self::Mixed(mixed);
    }

    fn make_mixed(&mut self, reason: PackedToMixedReason) {
        if matches!(self, Self::Record(_)) {
            self.make_record_mixed(RecordToMixedReason::GenericMutation);
            return;
        }
        let Self::Packed(storage) = self else {
            return;
        };
        crate::layout_stats::record_packed_to_mixed(reason);
        let entries = std::mem::take(&mut storage.values)
            .into_iter()
            .enumerate()
            .map(|(index, value)| ArrayEntry::new(ArrayKey::Int(index as i64), value))
            .collect::<Vec<_>>();
        // `ArrayKey` hashing/equality depend only on the key bytes; the
        // interior cells on `PhpString` memoize the hash and symbol id
        // without ever changing either relation (see `build_index`).
        #[allow(clippy::mutable_key_type)]
        let index = build_index(&entries);
        let mixed = MixedArrayStorage {
            storage_id: storage.storage_id,
            live_len: entries.len(),
            entries: entries.into_iter().map(Some).collect(),
            index,
            next_append_key: storage.next_append_key,
            internal_pointer: storage.internal_pointer,
            mutation_epoch: storage.mutation_epoch,
            cached_metadata: storage.cached_metadata,
        };
        *self = Self::Mixed(mixed);
    }

    fn find_index(&self, key: &ArrayKey) -> Option<usize> {
        match self {
            Self::Packed(storage) => packed_key_index(storage.values.len(), key),
            Self::Record(storage) => {
                let ArrayKey::String(key) = key else {
                    return None;
                };
                if key.symbol_id().is_some() {
                    crate::layout_stats::record_record_key_symbol_hit();
                }
                let slot = storage.shape.slot_by_key.get(key).copied();
                if slot.is_some() {
                    crate::layout_stats::record_record_slot_read();
                }
                slot.map(|slot| slot as usize)
            }
            Self::Mixed(storage) => storage.index.get(key).copied(),
        }
    }

    fn push_entry(&mut self, entry: ArrayEntry) {
        match self {
            Self::Record(storage) => {
                let ArrayKey::String(key) = &entry.key else {
                    unreachable!("record pushes carry string keys; ints convert to mixed first");
                };
                debug_assert!(!storage.shape.slot_by_key.contains_key(key));
                // Sole owner of a private (unregistered) shape: append in
                // place — O(1) per insert. `shape_id` stays: nothing guards
                // on it, and existing slot indices are append-stable.
                // Otherwise, large growing maps privatize (one O(len) copy,
                // O(1) appends afterwards) while small maps keep interned
                // shared shapes through the memoized transition chain.
                if let Some(shape) = Rc::get_mut(&mut storage.shape) {
                    let interned = PhpString::intern(key.as_bytes());
                    shape
                        .slot_by_key
                        .insert(interned.clone(), shape.keys.len() as u32);
                    shape.keys.push(interned);
                } else if storage.shape.keys.len() >= RECORD_SHAPE_PRIVATE_GROWTH_THRESHOLD {
                    storage.shape = record_shape_private_extended(&storage.shape, key);
                } else {
                    storage.shape = record_shape_extended(&storage.shape, key);
                }
                crate::layout_stats::record_record_slot_write();
                storage.cached_metadata.add_record_value(&entry.value);
                storage.values.push(entry.value);
            }
            Self::Packed(storage) => {
                debug_assert!(
                    matches!(&entry.key, ArrayKey::Int(key) if *key as usize == storage.values.len()),
                    "packed pushes carry the next sequential integer key"
                );
                crate::layout_stats::record_packed_values_storage_append();
                storage.cached_metadata.add_packed_value(&entry.value);
                storage.values.push(entry.value);
            }
            Self::Mixed(storage) => {
                let index = storage.entries.len();
                let old = storage.index.insert(entry.key.clone(), index);
                debug_assert!(old.is_none(), "mixed array index must be unique");
                storage.cached_metadata.add_entry(&entry);
                storage.entries.push(Some(entry));
                storage.live_len += 1;
            }
        }
        self.debug_assert_consistent();
    }

    fn replace_value(&mut self, index: usize, value: Value) -> Value {
        let old = std::mem::replace(self.value_at_mut(index), value);
        let new = self.value_at(index);
        let old_is_reference = matches!(old, Value::Reference(_));
        let new_is_reference = matches!(new, Value::Reference(_));
        let old_is_int = matches!(old, Value::Int(_));
        let new_is_int = matches!(new, Value::Int(_));
        self.metadata_mut().replace_value_flags(
            old_is_reference,
            new_is_reference,
            old_is_int,
            new_is_int,
        );
        old
    }

    fn remove_index(&mut self, index: usize) -> (ArrayKey, Value) {
        match self {
            Self::Record(_) => {
                unreachable!("record arrays convert to mixed before element removal")
            }
            Self::Packed(storage) => {
                let value = storage.values.remove(index);
                storage.cached_metadata.remove_packed_value(&value);
                self.debug_assert_consistent();
                (ArrayKey::Int(index as i64), value)
            }
            Self::Mixed(storage) => {
                let entry = storage.entries[index]
                    .take()
                    .expect("mixed index must point to a live slot");
                storage.cached_metadata.remove_entry(&entry);
                storage.index.remove(&entry.key);
                storage.live_len -= 1;
                self.debug_assert_consistent();
                (entry.key, entry.value)
            }
        }
    }

    fn debug_assert_consistent(&self) {
        debug_assert_eq!(self.metadata().len, self.len());
        match self {
            Self::Packed(storage) => {
                debug_assert_eq!(
                    self.metadata(),
                    ArrayCachedMetadata::from_record_values_without_counter(&storage.values, None,)
                );
            }
            Self::Record(storage) => {
                debug_assert_eq!(storage.shape.keys.len(), storage.values.len());
                debug_assert_eq!(
                    self.metadata(),
                    ArrayCachedMetadata::from_record_values_without_counter(
                        &storage.values,
                        Some(storage.shape.keys.len()),
                    )
                );
            }
            Self::Mixed(storage) => {
                debug_assert_eq!(
                    self.metadata(),
                    ArrayCachedMetadata::from_entry_iter_for_debug(
                        storage.entries.iter().filter_map(Option::as_ref)
                    )
                );
                debug_assert_eq!(storage.index.len(), storage.live_len);
                for (index, entry) in storage.entries.iter().enumerate() {
                    if let Some(entry) = entry {
                        debug_assert_eq!(storage.index.get(&entry.key), Some(&index));
                    }
                }
            }
        }
    }
}

fn packed_key_index(len: usize, key: &ArrayKey) -> Option<usize> {
    let ArrayKey::Int(index) = key else {
        return None;
    };
    let index = usize::try_from(*index).ok()?;
    (index < len).then_some(index)
}

/// `ArrayKey` is a stable map key despite clippy's `mutable_key_type` view:
/// its `Hash`/`Eq` are pure functions of the key bytes, and the interior
/// cells on `PhpString` only memoize that hash (and an interned symbol id
/// that never overrides byte equality). Mutating a string always separates
/// its storage first, so a key already inside a map can never change.
#[allow(clippy::mutable_key_type)]
/// Finishing hasher for keys that already carry a cached stable 64-bit hash
/// (`PhpString`, `ArrayKey`). The default SipHash re-mixes those 8 bytes per
/// map operation, which dominates registry-style insert loops; one
/// multiply-xor round keeps bucket distribution without that cost. The inner
/// FNV-1a byte hash is unseeded either way, so this changes no collision
/// resistance properties.
#[derive(Clone, Copy, Default)]
pub(crate) struct StableKeyHasher(u64);

impl std::hash::Hasher for StableKeyHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        // Fallback for non-u64 writes (i64 int keys write via write_i64).
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        }
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        let mixed = value.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        self.0 = mixed ^ (mixed >> 32);
    }

    #[inline]
    fn write_i64(&mut self, value: i64) {
        self.write_u64(value as u64);
    }
}

/// `BuildHasher` for [`StableKeyHasher`].
#[derive(Clone, Copy, Default)]
pub(crate) struct StableKeyState;

impl std::hash::BuildHasher for StableKeyState {
    type Hasher = StableKeyHasher;

    #[inline]
    fn build_hasher(&self) -> Self::Hasher {
        StableKeyHasher::default()
    }
}

/// Hash map keyed by stable-hash-carrying keys.
pub(crate) type StableKeyMap<K, V> = HashMap<K, V, StableKeyState>;

#[allow(clippy::mutable_key_type)] // ArrayKey hash/eq are byte-pure.
fn build_index(entries: &[ArrayEntry]) -> StableKeyMap<ArrayKey, usize> {
    let mut index = StableKeyMap::with_capacity_and_hasher(entries.len(), StableKeyState);
    for (entry_index, entry) in entries.iter().enumerate() {
        let old = index.insert(entry.key.clone(), entry_index);
        debug_assert!(old.is_none(), "mixed array contains duplicate key");
    }
    index
}

/// Copy-on-write ordered PHP array facade.
///
/// Cloning a `PhpArray` shares immutable storage. Mutating methods call
/// `separate_for_write` through `storage_mut`, so by-value assignment shares
/// until the first write while true PHP references still write through their
/// owning slot/reference cell.
#[derive(Debug)]
pub struct PhpArray {
    storage: Rc<ArrayStorage>,
}

impl PartialEq for PhpArray {
    fn eq(&self, other: &Self) -> bool {
        self.storage == other.storage
    }
}

impl Eq for PhpArray {}

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
    id: u64,
    storage: Weak<ArrayStorage>,
}

impl WeakArrayHandle {
    /// Returns the process-local debug ID for this handle.
    #[must_use]
    pub const fn id(&self) -> u64 {
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
                storage_id: next_array_storage_id(),
                values: Vec::new(),
                next_append_key: None,
                internal_pointer: None,
                mutation_epoch: 0,
                cached_metadata: ArrayCachedMetadata::default(),
            })),
        }
    }

    /// Creates a packed array with integer keys starting at zero.
    #[must_use]
    pub fn from_packed(elements: Vec<Value>) -> Self {
        let len = elements.len();
        crate::layout_stats::record_packed_values_storage_array();
        let cached_metadata = ArrayCachedMetadata::from_packed_values(&elements);
        Self {
            storage: Rc::new(ArrayStorage::Packed(PackedArrayStorage {
                storage_id: next_array_storage_id(),
                values: elements,
                next_append_key: (len > 0).then(|| i64::try_from(len).ok()).flatten(),
                internal_pointer: (len > 0).then_some(0),
                mutation_epoch: len as u64,
                cached_metadata,
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

    /// Consumes the array into owned `(key, value)` pairs. A handle that
    /// solely owns its storage moves the values out without cloning;
    /// shared storage falls back to cloning each pair.
    #[must_use]
    pub fn into_pairs(self) -> Vec<(ArrayKey, Value)> {
        match Rc::try_unwrap(self.storage) {
            Ok(storage) => match storage {
                ArrayStorage::Packed(storage) => storage
                    .values
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| (ArrayKey::Int(index as i64), value))
                    .collect(),
                ArrayStorage::Record(storage) => {
                    let keys = storage.shape.keys.clone();
                    storage
                        .values
                        .into_iter()
                        .zip(keys)
                        .map(|(value, key)| (ArrayKey::String(key), value))
                        .collect()
                }
                ArrayStorage::Mixed(storage) => storage
                    .entries
                    .into_iter()
                    .flatten()
                    .map(ArrayEntry::into_key_value)
                    .collect(),
            },
            Err(storage) => Self { storage }
                .iter()
                .map(|(key, value)| (key, value.clone()))
                .collect(),
        }
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
        self.storage.metadata().contains_references()
    }

    /// Returns a cheap direct-element summary.
    #[must_use]
    pub fn element_summary_fast(&self) -> PhpArrayElementSummary {
        self.storage.metadata().element_summary()
    }

    /// Returns a cheap key-shape summary.
    #[must_use]
    pub fn key_kind_summary_fast(&self) -> PhpArrayKeyKindSummary {
        self.storage
            .metadata()
            .key_kind_summary(self.is_packed_fast())
    }

    /// Returns true when string keys look numeric but intentionally remain
    /// strings under PHP key-normalization rules.
    #[must_use]
    pub fn has_numeric_string_key_ambiguity_fast(&self) -> bool {
        self.storage.metadata().has_numeric_string_key_ambiguity()
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

    /// Key/value pair at an insertion-order position; the value is a
    /// PHP-visible copy. Used by handle-based foreach iteration.
    #[must_use]
    pub fn pair_at(&self, index: usize) -> Option<(ArrayKey, Value)> {
        if let ArrayStorage::Mixed(storage) = self.storage.as_ref() {
            let entry = storage
                .entries
                .iter()
                .filter_map(Option::as_ref)
                .nth(index)?;
            let _source = enter_default_layout_source_family(SOURCE_FOREACH_VALUE);
            return Some((entry.key.clone(), entry.value.clone()));
        }
        let key = self.storage.key_at(index)?;
        let _source = enter_default_layout_source_family(SOURCE_FOREACH_VALUE);
        let value = self.storage.get_value(index)?.clone();
        Some((key, value))
    }

    /// Advances a storage cursor and returns the next insertion-order pair.
    /// Mixed-array cursors skip tombstones in one forward pass, avoiding the
    /// quadratic cost of repeatedly resolving a logical position.
    pub fn next_pair_at_cursor(&self, cursor: &mut usize) -> Option<(ArrayKey, Value)> {
        if let ArrayStorage::Mixed(storage) = self.storage.as_ref() {
            while let Some(entry) = storage.entries.get(*cursor) {
                *cursor += 1;
                let Some(entry) = entry else {
                    continue;
                };
                let _source = enter_default_layout_source_family(SOURCE_FOREACH_VALUE);
                return Some((entry.key.clone(), entry.value.clone()));
            }
            return None;
        }
        let pair = self.pair_at(*cursor)?;
        *cursor += 1;
        Some(pair)
    }

    /// True when this array currently uses shaped record storage.
    #[must_use]
    pub fn is_record_storage(&self) -> bool {
        matches!(self.storage.as_ref(), ArrayStorage::Record(_))
    }

    /// Record-storage slot index for a string key, when this array uses
    /// shaped record storage.
    #[must_use]
    pub fn record_slot_for_symbol(&self, key: &PhpString) -> Option<u32> {
        let ArrayStorage::Record(storage) = self.storage.as_ref() else {
            return None;
        };
        storage.shape.slot_by_key.get(key).copied()
    }

    /// Direct record-slot read by string key; `None` when the array is not
    /// record-shaped or the key is absent.
    #[must_use]
    pub fn record_get_symbol(&self, key: &PhpString) -> Option<&Value> {
        let ArrayStorage::Record(storage) = self.storage.as_ref() else {
            return None;
        };
        if key.symbol_id().is_some() {
            crate::layout_stats::record_record_key_symbol_hit();
        }
        let slot = storage.shape.slot_by_key.get(key).copied()?;
        crate::layout_stats::record_record_slot_read();
        storage.values.get(slot as usize)
    }

    /// Direct record-slot overwrite by string key. Returns false when the
    /// array is not record-shaped or the key is absent; callers fall back
    /// to the generic insert path.
    pub fn record_set_symbol(&mut self, key: &PhpString, value: Value) -> bool {
        if self.record_slot_for_symbol(key).is_none() {
            return false;
        }
        let storage = self.storage_mut_for(PhpArrayWriteIntent::NestedDimensionWrite);
        let ArrayStorage::Record(record) = &*storage else {
            return false;
        };
        let Some(slot) = record.shape.slot_by_key.get(key).copied() else {
            return false;
        };
        crate::layout_stats::record_record_slot_write();
        storage.replace_value(slot as usize, value);
        bump_mutation_epoch(storage);
        true
    }

    fn string_keys_share_storage_fast(&self) -> bool {
        self.storage.metadata().string_keys_share_storage()
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
    pub fn gc_debug_id(&self) -> u64 {
        self.storage.storage_id()
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
        let _ = self.storage_mut_for(PhpArrayWriteIntent::VariableWrite);
    }

    /// Central copy-on-write preparation point for PHP array mutations.
    pub fn prepare_for_write(&mut self, intent: PhpArrayWriteIntent) {
        let _ = self.storage_mut_for(intent);
    }

    /// Inserts or overwrites a key. Existing-key overwrites preserve insertion
    /// order.
    pub fn insert(&mut self, key: ArrayKey, value: Value) -> Option<Value> {
        let intent = if matches!(value, Value::Reference(_)) {
            PhpArrayWriteIntent::BindReferenceElement
        } else {
            PhpArrayWriteIntent::NestedDimensionWrite
        };
        let storage = self.storage_mut_for(intent);
        bump_append_key(storage, &key);
        if let Some(index) = storage.find_index(&key) {
            bump_mutation_epoch(storage);
            return Some(storage.replace_value(index, value));
        }
        let old_len = storage.len();
        let remains_packed =
            storage.is_packed() && matches!(key, ArrayKey::Int(value) if value == old_len as i64);
        if !remains_packed {
            if storage.is_record() {
                match &key {
                    ArrayKey::Int(_) => {
                        storage.make_record_mixed(RecordToMixedReason::IntKey);
                    }
                    ArrayKey::String(name) if array_key_has_numeric_string_ambiguity(name) => {
                        storage.make_record_mixed(RecordToMixedReason::AmbiguousKey);
                    }
                    ArrayKey::String(_) => {}
                }
            } else if storage.is_packed()
                && old_len == 0
                && matches!(&key, ArrayKey::String(name) if !array_key_has_numeric_string_ambiguity(name))
            {
                storage.promote_empty_to_record();
            } else {
                storage.make_mixed(match key {
                    ArrayKey::String(_) => PackedToMixedReason::StringKey,
                    ArrayKey::Int(_) => PackedToMixedReason::NonSequentialIntKey,
                });
            }
        }
        storage.push_entry(ArrayEntry::new(key, value));
        if storage.internal_pointer().is_none() {
            storage.set_internal_pointer(storage.first_index());
        }
        bump_mutation_epoch(storage);
        None
    }

    /// Returns whether an implicit append has a free integer key.
    #[must_use]
    pub fn can_append(&self) -> bool {
        let next = self.storage.next_append_key().unwrap_or(0);
        next != i64::MAX || self.storage.find_index(&ArrayKey::Int(i64::MAX)).is_none()
    }

    /// Appends with the next integer key, returning PHP's overflow condition.
    pub fn try_append(&mut self, value: Value) -> Result<ArrayKey, PhpArrayAppendError> {
        if !self.can_append() {
            return Err(PhpArrayAppendError);
        }
        let intent = if matches!(value, Value::Reference(_)) {
            PhpArrayWriteIntent::BindReferenceElement
        } else {
            PhpArrayWriteIntent::Append
        };
        let storage = self.storage_mut_for(intent);
        if storage.is_record() {
            storage.make_record_mixed(RecordToMixedReason::IntKey);
        }
        let key = ArrayKey::Int(storage.next_append_key().unwrap_or(0));
        let old_len = storage.len();
        let remains_packed =
            storage.is_packed() && matches!(key, ArrayKey::Int(value) if value == old_len as i64);
        bump_append_key(storage, &key);
        if !remains_packed {
            storage.make_mixed(PackedToMixedReason::AppendKeyGap);
        }
        storage.push_entry(ArrayEntry::new(key.clone(), value));
        if storage.internal_pointer().is_none() {
            storage.set_internal_pointer(storage.first_index());
        }
        bump_mutation_epoch(storage);
        Ok(key)
    }

    /// Appends with the next integer key.
    ///
    /// PHP execution paths that must surface the overflow exception use
    /// [`Self::try_append`]. This compatibility helper leaves the array
    /// unchanged on overflow and never panics on user-controlled input.
    pub fn append(&mut self, value: Value) -> ArrayKey {
        self.try_append(value).unwrap_or(ArrayKey::Int(i64::MAX))
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
        match self.storage.as_ref() {
            ArrayStorage::Packed(storage) => {
                let index = packed_key_index(storage.values.len(), key)?;
                crate::layout_stats::record_array_packed_direct_get();
                crate::layout_stats::record_packed_values_storage_read();
                storage.values.get(index)
            }
            ArrayStorage::Record(storage) => {
                let ArrayKey::String(name) = key else {
                    return None;
                };
                if name.symbol_id().is_some() {
                    crate::layout_stats::record_record_key_symbol_hit();
                }
                let slot = storage.shape.slot_by_key.get(name).copied()?;
                crate::layout_stats::record_record_slot_read();
                storage.values.get(slot as usize)
            }
            ArrayStorage::Mixed(storage) => {
                let index = storage.index.get(key).copied()?;
                crate::layout_stats::record_array_mixed_indexed_get();
                storage
                    .entries
                    .get(index)
                    .and_then(Option::as_ref)
                    .map(ArrayEntry::value)
            }
        }
    }

    /// Returns a mutable value by normalized key without exposing storage.
    pub fn get_mut(&mut self, key: &ArrayKey) -> Option<PhpArrayValueMut<'_>> {
        let storage = self.storage_mut_for(PhpArrayWriteIntent::NestedDimensionWrite);
        let index = storage.find_index(key)?;
        let value = storage.value_at(index);
        let old_is_reference = matches!(value, Value::Reference(_));
        let old_is_int = matches!(value, Value::Int(_));
        bump_mutation_epoch(storage);
        Some(PhpArrayValueMut {
            storage,
            index,
            old_is_reference,
            old_is_int,
        })
    }

    /// Removes a value by normalized key.
    pub fn remove(&mut self, key: &ArrayKey) -> Option<Value> {
        let storage = self.storage_mut_for(PhpArrayWriteIntent::Remove);
        let index = storage.find_index(key)?;
        // Removing a non-tail packed element must preserve the remaining
        // keys, so the array leaves values-only storage before the removal
        // renumbers positions; record storage keeps its shape immutable and
        // converts for any element removal.
        if storage.is_record() {
            storage.make_record_mixed(RecordToMixedReason::GenericMutation);
        } else if storage.is_packed() && index + 1 != storage.len() {
            storage.make_mixed(PackedToMixedReason::UnsetHole);
        }
        let (_removed_key, value) = storage.remove_index(index);
        adjust_pointer_after_remove(storage, index);
        storage.compact_mixed_if_needed();
        bump_mutation_epoch(storage);
        Some(value)
    }

    /// Removes and returns the last element, mirroring PHP's `array_pop`
    /// adjustment of the next auto-index: when the removed key is the most
    /// recent auto-index (`next_append_key - 1`), the next index is decremented
    /// so a following `[]=` reuses it (e.g. popping `-2` from `[-2 => x]` makes
    /// the next append `-2` again).
    pub fn pop(&mut self) -> Option<Value> {
        let last_key = self.storage.key_at(self.storage.last_index()?)?;
        let previous_next = self.storage.next_append_key();
        let value = self.remove(&last_key);
        if let ArrayKey::Int(key) = last_key
            && previous_next == Some(key.saturating_add(1))
        {
            self.storage_mut_for(PhpArrayWriteIntent::PointerMutation)
                .set_next_append_key(Some(key));
        }
        value
    }

    /// Returns the current internal-pointer value.
    #[must_use]
    pub fn pointer_value(&self) -> Option<Value> {
        self.storage
            .internal_pointer()
            .and_then(|index| self.storage.get_value(index))
            .cloned()
    }

    /// Returns the current internal-pointer key.
    #[must_use]
    pub fn pointer_key(&self) -> Option<ArrayKey> {
        self.storage
            .internal_pointer()
            .and_then(|index| self.storage.key_at(index))
    }

    /// Moves the internal pointer to the first element.
    pub fn reset_pointer(&mut self) -> Option<Value> {
        let storage = self.storage_mut_for(PhpArrayWriteIntent::PointerMutation);
        if storage.is_empty() {
            storage.set_internal_pointer(None);
            return None;
        }
        let first = storage.first_index()?;
        storage.set_internal_pointer(Some(first));
        storage.get_value(first).cloned()
    }

    /// Moves the internal pointer to the last element.
    pub fn end_pointer(&mut self) -> Option<Value> {
        let storage = self.storage_mut_for(PhpArrayWriteIntent::PointerMutation);
        let last = storage.last_index()?;
        storage.set_internal_pointer(Some(last));
        storage.get_value(last).cloned()
    }

    /// Advances the internal pointer by one element.
    pub fn next_pointer(&mut self) -> Option<Value> {
        let storage = self.storage_mut_for(PhpArrayWriteIntent::PointerMutation);
        let current = storage.internal_pointer()?;
        let Some(next) = storage.next_index(current) else {
            storage.set_internal_pointer(None);
            return None;
        };
        storage.set_internal_pointer(Some(next));
        storage.get_value(next).cloned()
    }

    /// Moves the internal pointer one element backwards.
    pub fn prev_pointer(&mut self) -> Option<Value> {
        let storage = self.storage_mut_for(PhpArrayWriteIntent::PointerMutation);
        let Some(current) = storage.internal_pointer() else {
            let last = storage.last_index()?;
            storage.set_internal_pointer(Some(last));
            return storage.get_value(last).cloned();
        };
        let Some(previous) = storage.previous_index(current) else {
            storage.set_internal_pointer(None);
            return None;
        };
        storage.set_internal_pointer(Some(previous));
        storage.get_value(previous).cloned()
    }

    /// Iterates in insertion order. Packed arrays synthesize their
    /// sequential integer keys, so keys are yielded by value.
    pub fn iter(&self) -> PhpArrayIter<'_> {
        self.storage.iter_pairs()
    }

    /// Returns packed elements only when the keys are exactly `0..len`.
    #[must_use]
    pub fn packed_elements(&self) -> Option<Vec<&Value>> {
        if self.is_packed_fast() {
            return Some(self.packed_values_fast()?.collect());
        }
        crate::layout_stats::record_array_linear_scan_fallback();
        let storage = match self.storage.as_ref() {
            ArrayStorage::Packed(storage) => return Some(storage.values.iter().collect()),
            ArrayStorage::Record(_) => return None,
            ArrayStorage::Mixed(storage) => storage,
        };
        let mut elements = Vec::with_capacity(storage.live_len);
        for (index, entry) in storage
            .entries
            .iter()
            .filter_map(Option::as_ref)
            .enumerate()
        {
            if entry.key != ArrayKey::Int(index as i64) {
                return None;
            }
            elements.push(&entry.value);
        }
        Some(elements)
    }

    /// Iterates packed values without allocation when tracked metadata proves
    /// keys are exactly `0..len`.
    #[must_use]
    pub fn packed_values_fast(&self) -> Option<PackedArrayValues<'_>> {
        let ArrayStorage::Packed(storage) = self.storage.as_ref() else {
            return None;
        };
        crate::layout_stats::record_packed_values_storage_read();
        Some(PackedArrayValues {
            values: storage.values.iter(),
        })
    }

    /// Returns one packed element only when the keys are exactly `0..len`.
    #[must_use]
    pub fn packed_element(&self, index: usize) -> Option<&Value> {
        crate::layout_stats::record_array_linear_scan_fallback();
        match self.storage.as_ref() {
            ArrayStorage::Packed(storage) => storage.values.get(index),
            ArrayStorage::Record(_) => None,
            ArrayStorage::Mixed(storage) => {
                for (entry_index, entry) in storage
                    .entries
                    .iter()
                    .filter_map(Option::as_ref)
                    .enumerate()
                {
                    if entry.key != ArrayKey::Int(entry_index as i64) {
                        return None;
                    }
                }
                storage
                    .entries
                    .iter()
                    .filter_map(Option::as_ref)
                    .nth(index)
                    .map(ArrayEntry::value)
            }
        }
    }

    /// Returns one packed element using only tracked metadata.
    #[must_use]
    pub fn packed_element_fast(&self, index: usize) -> Option<&Value> {
        if !self.is_packed_fast() {
            return None;
        }
        crate::layout_stats::record_packed_values_storage_read();
        self.storage.get_value(index)
    }

    fn storage_mut_for(&mut self, _intent: PhpArrayWriteIntent) -> &mut ArrayStorage {
        if self.is_shared() {
            crate::layout_stats::record_cow_separation();
            crate::layout_stats::sample_cow_separation_backtrace(Rc::strong_count(&self.storage));
            // Attribute the element deep-copy performed by `Rc::make_mut` to
            // the separation itself instead of the outer operation family.
            let _source = crate::layout_stats::enter_layout_source_family(
                crate::layout_stats::SOURCE_COW_SEPARATION_CONTENTS,
            );
            let storage = Rc::make_mut(&mut self.storage);
            storage.set_storage_id(next_array_storage_id());
            return storage;
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
    if matches!(storage, ArrayStorage::Mixed(_)) {
        let pointer = if storage.is_empty() {
            None
        } else if pointer == removed_index {
            storage.next_index(removed_index)
        } else {
            Some(pointer)
        };
        storage.set_internal_pointer(pointer);
        return;
    }
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
    fn array_append_at_exhausted_max_key_is_non_panicking_error() {
        let mut array = PhpArray::new();
        array.insert(ArrayKey::Int(i64::MAX), Value::Int(42));

        assert!(!array.can_append());
        assert_eq!(
            array.try_append(Value::Int(7)),
            Err(super::PhpArrayAppendError)
        );
        assert_eq!(array.len(), 1);
        assert_eq!(array.get(&ArrayKey::Int(i64::MAX)), Some(&Value::Int(42)));
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
    fn mixed_array_index_survives_overwrite_remove_cow_and_spread() {
        crate::layout_stats::reset_layout_stats();

        let mut array = PhpArray::new();
        array.insert(ArrayKey::String(PhpString::from("a")), Value::Int(1));
        array.insert(ArrayKey::String(PhpString::from("b")), Value::Int(2));
        array.insert(ArrayKey::String(PhpString::from("a")), Value::Int(3));
        assert_eq!(
            array.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>(),
            vec![
                ArrayKey::String(PhpString::from("a")),
                ArrayKey::String(PhpString::from("b")),
            ]
        );
        assert_eq!(
            array.get(&ArrayKey::String(PhpString::from("a"))),
            Some(&Value::Int(3))
        );
        assert_eq!(
            array.remove(&ArrayKey::String(PhpString::from("b"))),
            Some(Value::Int(2))
        );
        assert_eq!(array.get(&ArrayKey::String(PhpString::from("b"))), None);

        let mut copy = array.clone();
        copy.insert(ArrayKey::String(PhpString::from("c")), Value::Int(4));
        assert_eq!(array.get(&ArrayKey::String(PhpString::from("c"))), None);
        assert_eq!(
            copy.get(&ArrayKey::String(PhpString::from("c"))),
            Some(&Value::Int(4))
        );

        let mut spread = PhpArray::new();
        spread.append(Value::Int(10));
        spread.insert(ArrayKey::String(PhpString::from("a")), Value::Int(30));
        spread.insert(ArrayKey::String(PhpString::from("d")), Value::Int(40));
        copy.spread_extend(&spread);
        assert_eq!(copy.get(&ArrayKey::Int(0)), Some(&Value::Int(10)));
        assert_eq!(
            copy.get(&ArrayKey::String(PhpString::from("a"))),
            Some(&Value::Int(30))
        );
        assert_eq!(
            copy.get(&ArrayKey::String(PhpString::from("d"))),
            Some(&Value::Int(40))
        );

        if let ArrayStorage::Mixed(storage) = copy.storage.as_ref() {
            for (index, entry) in storage.entries.iter().enumerate() {
                if let Some(entry) = entry {
                    assert_eq!(storage.index.get(entry.key()), Some(&index));
                }
            }
        } else {
            panic!("copy should use indexed mixed storage");
        }

        let stats = crate::layout_stats::take_layout_stats();
        assert!(stats.array_mixed_indexed_gets >= 4, "{stats:?}");
        assert_eq!(stats.array_linear_scan_fallbacks, 0, "{stats:?}");
    }

    #[test]
    fn mixed_lookup_matches_interned_and_uninterned_string_keys() {
        let mut array = PhpArray::new();
        array.insert(ArrayKey::String(PhpString::intern(b"alpha")), Value::Int(1));
        array.insert(
            ArrayKey::String(PhpString::from_bytes(b"beta".to_vec())),
            Value::Int(2),
        );

        // Interned and plain keys with equal bytes must find the same entry
        // regardless of how the stored key was created.
        let interned_alpha = ArrayKey::String(PhpString::intern(b"alpha"));
        let plain_alpha = ArrayKey::String(PhpString::from_bytes(b"alpha".to_vec()));
        let interned_beta = ArrayKey::String(PhpString::intern(b"beta"));
        let plain_beta = ArrayKey::String(PhpString::from_bytes(b"beta".to_vec()));
        assert_eq!(array.get(&interned_alpha), Some(&Value::Int(1)));
        assert_eq!(array.get(&plain_alpha), Some(&Value::Int(1)));
        assert_eq!(array.get(&interned_beta), Some(&Value::Int(2)));
        assert_eq!(array.get(&plain_beta), Some(&Value::Int(2)));
        assert_eq!(
            array.get(&ArrayKey::String(PhpString::intern(b"gamma"))),
            None
        );
    }

    #[test]
    fn packed_get_uses_direct_index_and_mixed_transition_keeps_stable_slots() {
        crate::layout_stats::reset_layout_stats();

        let mut array = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(array.get(&ArrayKey::Int(1)), Some(&Value::Int(2)));
        assert_eq!(array.remove(&ArrayKey::Int(1)), Some(Value::Int(2)));
        assert_eq!(array.get(&ArrayKey::Int(2)), Some(&Value::Int(3)));

        if let ArrayStorage::Mixed(storage) = array.storage.as_ref() {
            assert_eq!(storage.index.get(&ArrayKey::Int(0)), Some(&0));
            assert_eq!(storage.index.get(&ArrayKey::Int(2)), Some(&2));
        } else {
            panic!("middle removal should transition to mixed storage");
        }

        let stats = crate::layout_stats::take_layout_stats();
        assert_eq!(stats.array_packed_direct_gets, 1, "{stats:?}");
        assert_eq!(stats.array_mixed_indexed_gets, 1, "{stats:?}");
    }

    #[test]
    fn mixed_deletion_compacts_tombstones_at_the_bounded_threshold() {
        let mut array = PhpArray::new();
        for index in 0..128 {
            array.insert(
                ArrayKey::String(PhpString::from_bytes(format!("key-{index}").into_bytes())),
                Value::Int(index),
            );
        }
        for index in 0..80 {
            assert_eq!(
                array.remove(&ArrayKey::String(PhpString::from_bytes(
                    format!("key-{index}").into_bytes(),
                ))),
                Some(Value::Int(index))
            );
        }

        let ArrayStorage::Mixed(storage) = array.storage.as_ref() else {
            panic!("removal from record storage should transition to mixed storage");
        };
        assert_eq!(storage.live_len, 48);
        assert!(storage.entries.len() <= storage.live_len + 31);
        assert_eq!(
            array.iter().next().map(|(key, _)| key),
            Some(ArrayKey::String(PhpString::from("key-80")))
        );
        assert_eq!(
            array.iter().last().map(|(key, _)| key),
            Some(ArrayKey::String(PhpString::from("key-127")))
        );
    }

    #[test]
    fn mixed_index_preserves_numeric_string_references_pointer_and_pop() {
        crate::layout_stats::reset_layout_stats();

        let cell = crate::ReferenceCell::new(Value::Int(42));
        let mut array = PhpArray::new();
        array.insert(ArrayKey::String(PhpString::from("a")), Value::Int(1));
        array.insert(
            ArrayKey::from_php_string(PhpString::from("1")),
            Value::Int(10),
        );
        array.insert(
            ArrayKey::from_php_string(PhpString::from("01")),
            Value::Reference(cell.clone()),
        );
        array.insert(ArrayKey::String(PhpString::from("z")), Value::Int(99));

        assert_eq!(array.get(&ArrayKey::Int(1)), Some(&Value::Int(10)));
        assert_eq!(
            array.get(&ArrayKey::String(PhpString::from("01"))),
            Some(&Value::Reference(cell))
        );
        assert!(array.contains_references_fast());

        assert_eq!(array.reset_pointer(), Some(Value::Int(1)));
        assert_eq!(array.next_pointer(), Some(Value::Int(10)));
        assert_eq!(
            array.remove(&ArrayKey::String(PhpString::from("a"))),
            Some(Value::Int(1))
        );
        assert_eq!(array.pointer_key(), Some(ArrayKey::Int(1)));
        assert_eq!(array.remove(&ArrayKey::Int(1)), Some(Value::Int(10)));
        assert_eq!(
            array.pointer_key(),
            Some(ArrayKey::String(PhpString::from("01")))
        );
        assert_eq!(array.pop(), Some(Value::Int(99)));
        assert_eq!(array.get(&ArrayKey::String(PhpString::from("z"))), None);
        assert!(matches!(
            array.get(&ArrayKey::String(PhpString::from("01"))),
            Some(Value::Reference(_))
        ));
        assert_eq!(
            array.pointer_key(),
            Some(ArrayKey::String(PhpString::from("01")))
        );

        if let ArrayStorage::Mixed(storage) = array.storage.as_ref() {
            assert_eq!(storage.live_len, 1);
            assert_eq!(
                storage.index.get(&ArrayKey::String(PhpString::from("01"))),
                storage.entries.iter().position(Option::is_some).as_ref()
            );
        } else {
            panic!("numeric-string key should keep mixed storage");
        }

        let stats = crate::layout_stats::take_layout_stats();
        assert!(stats.array_mixed_indexed_gets >= 3, "{stats:?}");
        assert_eq!(stats.array_linear_scan_fallbacks, 0, "{stats:?}");
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
    fn array_key_conversion_covers_runtime_value_types() {
        assert_eq!(ArrayKey::from_value(&Value::Int(4)), Some(ArrayKey::Int(4)));
        assert_eq!(
            ArrayKey::from_value(&Value::Bool(true)),
            Some(ArrayKey::Int(1))
        );
        assert_eq!(
            ArrayKey::from_value(&Value::Null),
            Some(ArrayKey::String(PhpString::from("")))
        );
        assert_eq!(
            ArrayKey::from_value(&Value::float(4.9)),
            Some(ArrayKey::Int(4))
        );
        assert_eq!(
            ArrayKey::from_value(&Value::String(PhpString::from("42"))),
            Some(ArrayKey::Int(42))
        );
        assert_eq!(
            ArrayKey::from_value(&Value::String(PhpString::from("042"))),
            Some(ArrayKey::String(PhpString::from("042")))
        );
        assert_eq!(
            ArrayKey::from_value(&Value::String(PhpString::from("+42"))),
            Some(ArrayKey::String(PhpString::from("+42")))
        );
        assert_eq!(
            ArrayKey::from_value(&Value::String(PhpString::from("-42"))),
            Some(ArrayKey::Int(-42))
        );
        assert_eq!(
            ArrayKey::from_value(&Value::String(PhpString::from("-0"))),
            Some(ArrayKey::String(PhpString::from("-0")))
        );
        assert_eq!(
            ArrayKey::from_value(&Value::String(PhpString::from("9223372036854775808"))),
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
    fn get_mut_guard_updates_cached_value_metadata_on_drop() {
        let mut array = PhpArray::from_packed(vec![Value::Int(1)]);
        assert!(!array.contains_references_fast());
        assert_eq!(array.element_summary_fast(), PhpArrayElementSummary::AllInt);

        {
            let mut slot = array.get_mut(&ArrayKey::Int(0)).expect("entry");
            *slot = Value::Reference(crate::ReferenceCell::new(Value::Int(1)));
        }

        assert!(array.contains_references_fast());
        assert_eq!(array.element_summary_fast(), PhpArrayElementSummary::Mixed);
    }

    #[test]
    fn cached_metadata_updates_without_recompute_on_repeated_reads() {
        crate::layout_stats::reset_layout_stats();

        let cell = crate::ReferenceCell::new(Value::Int(7));
        let mut array = PhpArray::new();
        array.insert(ArrayKey::String(PhpString::from("01")), Value::Int(1));
        array.insert(
            ArrayKey::String(PhpString::from("ref")),
            Value::Reference(cell),
        );

        for _ in 0..8 {
            let metadata = array.shape_metadata();
            assert!(metadata.contains_references);
            assert_eq!(
                metadata.key_kind_summary,
                PhpArrayKeyKindSummary::StringOnly
            );
            assert!(metadata.numeric_string_key_ambiguity);
            assert_eq!(array.element_summary_fast(), PhpArrayElementSummary::Mixed);
        }

        let stats = crate::layout_stats::take_layout_stats();
        assert_eq!(stats.array_metadata_recomputes, 0, "{stats:?}");
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
        // Record storage interns its shape keys, so string-key maps now
        // classify as interned records even when built from fresh strings.
        assert_eq!(
            record.shape_metadata().kind,
            PhpArrayShapeKind::InternedStringKeyRecord
        );

        let shared_key = PhpString::from("id");
        let mut interned_record = PhpArray::new();
        interned_record.insert(ArrayKey::String(shared_key.clone()), Value::Int(1));
        // The record shape holds an interner-backed copy of the key, so the
        // caller's handle no longer shares storage with the array itself.
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

    #[test]
    fn logical_storage_id_changes_only_when_cow_separates() {
        let original = PhpArray::from_packed(vec![Value::Int(1)]);
        let mut copy = original.clone();

        assert_eq!(original.gc_debug_id(), copy.gc_debug_id());
        copy.append(Value::Int(2));
        assert_ne!(original.gc_debug_id(), copy.gc_debug_id());
        assert_eq!(original.get(&ArrayKey::Int(1)), None);
    }
}
