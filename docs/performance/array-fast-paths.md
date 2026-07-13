# Performance Array Fast Paths

The performance layer keeps the Runtime semantics ordered-vector array representation and adds
conservative packed-shape metadata around it. The VM still observes PHP array
semantics through `PhpArray` APIs; no caller sees a second representation.

## Packed Shape Metadata

`PhpArray` tracks `packed_len: Option<usize>` and `mutation_epoch: u64` inside
request-local array storage. `Some(len)` means the entries are known to be
integer keys `0..len` in insertion order. `None` means mixed or unknown. The
exact public packed facade still validates keys when requested, so metadata is
only used for fast-path decisions.

The VM consumes `PhpArray::packed_metadata()`, which exposes:

- `kind`: `PackedList` or `MixedHash`;
- `is_shared`: whether copy-on-write storage has multiple owners;
- `contains_references`: whether any direct slot stores a PHP reference;
- `mutation_epoch`: a structural/content write epoch;
- `packed_len`: the proven packed length, if any.

The metadata remains packed for:

- appending the next sequential integer key,
- overwriting an existing packed integer key,
- appending reference values without changing copy-on-write behavior.

It transitions to mixed for:

- non-sequential integer keys,
- string keys,
- unset holes,
- appending after the last element was unset when PHP's next append key skips a
  visible hole.

The transition is intentionally one-way for fast metadata. A later deletion can
make visible keys list-shaped again, and the exact facade will detect that, but
VM fast paths require tracked packed metadata.

## VM Fast Paths and Counters

The VM records the following counters when counter collection is enabled:

- `array_packed_append_fast_path_hits`
- `array_packed_read_fast_path_hits`
- `array_sequential_foreach_fast_path_hits`
- `packed_fetch_fast_hits`
- `packed_fetch_bounds_fallbacks`
- `packed_fetch_layout_fallbacks`
- `packed_append_fast_hits`
- `packed_foreach_fast_hits`
- `cow_or_reference_fallbacks`
- `array_count_fast_path_hits`
- `array_packed_to_mixed_transitions`

Packed integer reads reuse the existing quickening path and now require
`packed_metadata()` plus `packed_element_fast`, so a quickened read cannot pass
on mixed or unknown shape and records bounds or layout fallback counters for
installed guard misses. Reference-containing arrays tail-call the generic fetch
and record `cow_or_reference_fallbacks`; shared read-only arrays remain eligible
because operand reads clone array handles.

Packed appends record hits only when the local array remains packed and the
write did not require COW/reference fallback. By-value `foreach` snapshot
creation records `packed_foreach_fast_hits` only for packed arrays without
direct reference elements; by-reference foreach remains on the generic local
array path. `count($array)` records a hit for non-recursive array counts before
dispatching to the existing builtin.

## Key Normalization Cache

No key-normalization cache is added in this scope. String key normalization is
pure, but the runtime does not yet expose a stable string-intern identity plus a
shape or request epoch that would make such a cache invalidation-proof. Adding a
cache without that contract would risk stale normalization across dynamically
created strings while providing little benefit for the current hot paths.

## Validation Coverage

Runtime tests cover packed metadata for sequential append, overwrite,
non-sequential integer keys, string keys, unset holes, append after tail unset,
references, and exact packed facade behavior.

VM tests cover packed append/read/foreach/count counters, bounds fallback,
layout fallback, COW/reference fallback, packed-to-mixed transitions, reference
element preservation, and foreach mutation order.
