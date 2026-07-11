# Runtime-memory safety audit

Ledger of every unsafe surface in `php_runtime::runtime_memory` (the only
module exempt from the crate's `#![deny(unsafe_code)]`; see ADR 0020).
Each entry names the invariants that make the unsafe sound and the tests
that exercise them. Update this file in the same commit as any change to
the module's unsafe code.

## CompactBytes

Single-allocation byte storage: one heap block holding a header
(refcount, cached hash cell, interned-symbol cell, length) followed
directly by the bytes. The public API is entirely safe; `CompactBytes`
behaves like `Rc<[u8]>` with two lazily-cached header fields.

Invariants:

- The allocation is created with the layout of `Header` extended by
  `len` bytes with the header's alignment; the same layout is recomputed
  from the stored `len` for deallocation.
- `ptr` is non-null, points at a live `Header`, and is only produced by
  `CompactBytes::from_slice` or `CompactBytes::from_parts`; the bytes
  pointer is derived by offsetting past the header within the same
  allocation.
- `from_parts` sizes the block as the sum of the part lengths and copies
  each part into a disjoint tail window whose offset is the sum of the
  preceding parts' lengths, so the copies exactly tile the `len`-byte
  tail; the result carries fresh hash/symbol cells (never inherited from
  the sources).
- The refcount is a `Cell<usize>`: the type is `!Send`/`!Sync` (enforced
  by a `PhantomData<Rc<()>>` marker), so counts never race.
- `clone` increments the refcount; `drop` decrements and frees on zero.
  The count starts at one; overflow aborts before wrapping.
- Byte content is immutable for the allocation's lifetime; mutation
  happens by building a new allocation (copy-on-write stays in the safe
  caller). Only the header's hash/symbol cells mutate, and they are
  plain `Cell`s on a single thread.

- `unique_bytes_mut` returns mutable bytes only for a uniquely-owned
  allocation (asserted), through an exclusive borrow — no other handle or
  outstanding byte reference can alias the returned slice.

Tests: construction/round-trip (empty, small, large), clone/drop
refcount behavior including interleavings, hash/symbol cell caching,
identity (`ptr_eq`/`addr`) and unique-mutation contracts, and a Miri run
via `safety-audit-smoke`.
