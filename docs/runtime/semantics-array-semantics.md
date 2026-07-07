# Runtime semantics Array Semantics

Runtime semantics arrays expose PHP-compatible key normalization, append-index behavior,
and insertion order. Packed versus mixed storage is an internal optimization
boundary only; no observable VM behavior may depend on which representation is
used.

## Key Normalization

Array keys at the VM boundary are normalized to `ArrayKey::Int(i64)` or
`ArrayKey::String(PhpString)`.

- Integer keys remain integer keys.
- Boolean keys normalize to `0` or `1`.
- `null` normalizes to the empty string key.
- Float keys truncate toward zero in the currently documented runtime subset.
- Decimal integer strings without a leading plus and without leading zeroes
  normalize to integer keys.
- Negative decimal integer strings normalize to integer keys, except `"-0"`.
- Strings with a leading plus, leading zeroes, non-decimal bytes, or values
  outside the Rust `i64` range remain string keys.

## Append Index

The append cursor is derived from integer keys only. An empty array appends at
key `0`. Inserting or appending integer key `n` advances the next append key to
`n + 1` when that is greater than the existing append cursor. This includes
negative integer keys, so inserting `-5` into an otherwise empty array makes the
next append key `-4`.

Removing an element does not rewind the append cursor. Overwriting an existing
key preserves that key's original insertion position.

## Ordering and Storage

Iteration order is insertion order. Overwrites do not move an entry, and
deletions remove only the targeted entry. A packed representation is valid only
when integer keys are exactly `0..len`; mixed storage must preserve the same
visible order and lookup behavior.

## Element References and Foreach

Direct array-element references are executable for covered dimension lvalues.
The VM separates the array payload before binding the element to a
`ReferenceCell`, so later writes through the reference update the selected
element without mutating unrelated by-value COW copies.

By-value foreach over arrays iterates a snapshot of insertion-order key/value
pairs. By-reference foreach is covered for local arrays and binds the target
value variable to each element reference in sequence. Temporary by-reference
sources and the full PHP mutation matrix are still explicit known gaps.

Object iteration is not implemented by converting objects to arrays. Plain
objects, `Iterator`, and `IteratorAggregate` use VM iteration sources described
in `docs/runtime/semantics-foreach-semantics.md`.

The architecture decision is recorded in
`docs/runtime/semantics-array-semantics.md`.

## Public API Surface

standard library should treat `php_runtime::PhpArray`, `ArrayKey`, `ArrayEntry`, and the
mutation APIs `insert`, `append`, `get_mut`, and `remove` as the runtime array
boundary. Packed versus mixed storage must remain internal.

performance-sensitive standard library work should focus on append-heavy arrays,
snapshot allocation for foreach, reference-cell storage in array entries, and
key-normalization costs. Optimizations must not make representation details
observable.

## Known Gaps

Integer overflow at the append cursor boundary is intentionally documented as a
known gap for Runtime semantics. The current implementation uses `i64` keys and saturates
the internal next-append cursor at `i64::MAX`; PHP 8.5.7 behavior around
platform integer limits must be handled behind a dedicated overflow diagnostic
before it is considered complete.

Platform differences are limited to integer-width behavior. The Runtime semantics target
is PHP 8.5.7 on a 64-bit reference build, and fixtures avoid requiring
32-bit-compatible integer-key behavior.
