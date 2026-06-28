# Runtime semantics Reference and Copy-on-Write Model

Runtime semantics separates PHP values from storage locations so references, array
elements, object properties, and VM temporaries can evolve without changing the
frontend or IR contracts.

## Core Terms

- `Value`: the effective PHP value, such as null, bool, int, string, array,
  object, callable, or an explicit `ReferenceCell` value used only at reference
  boundaries.
- `Slot`: a writable storage location for variables, properties, or array
  elements. A slot either owns an ordinary `Value` or points at a
  `ReferenceCell`.
- `ReferenceCell`: shared alias storage. Assigning through any slot bound to
  the cell updates the value observed through every other bound slot.
- `TempValue`: a VM temporary/register value. Temporaries snapshot effective
  values and are not referenceable storage locations.
- `PhpString`: byte-string payload with shared storage. Cloning shares bytes;
  write APIs call `separate_for_write` before mutation.
- `PhpArray`: ordered-map payload with shared storage. Cloning shares the
  private storage enum; mutating APIs call `separate_for_write` before changing
  entries, append metadata, or internal-pointer state.

## Invariants

- Reading a `Slot` dereferences `ReferenceCell` storage and returns the
  effective `Value`.
- Writing a `Slot` writes through `ReferenceCell` storage when the slot is an
  alias; otherwise it replaces the slot's owned value.
- Creating a reference from a by-value slot converts that slot into a
  `ReferenceCell` and returns the cell for the aliasing target.
- Binding a slot to an existing `ReferenceCell` makes the slot an alias of the
  same storage.
- Writing a `Value::Reference` into a `TempValue` dereferences the cell first.
  The temporary is a snapshot and later mutations to the cell do not mutate the
  temporary.
- Mutating a `TempValue` mutates only that temporary's private value. It must
  not write through a `ReferenceCell`.
- Normal by-value assignment of strings and arrays clones the value handle and
  may share payload storage.
- Mutating a shared string or array must separate that payload before the write.
  The original by-value copy remains observable unchanged.
- Mutating through a `Slot::Reference` still writes the separated result back
  into the `ReferenceCell`, so every alias sees the updated effective value.
- COW sharing is an optimization boundary for value payloads, not an aliasing
  mechanism. Only `ReferenceCell` creates PHP reference identity.
- `unset($name)` removes that local name's slot binding. If the slot was bound
  to a `ReferenceCell`, the cell and other aliases remain alive and retain
  their effective value.
- Rebinding a local reference replaces only the target slot's binding. Existing
  aliases to the previous cell keep pointing at the previous cell.

## Copy-on-Write Status

Arrays and strings now use shared payload storage with separation-on-write.
Array writes are covered through `PhpArray::insert`, `append`, `get_mut`, and
`remove`, which are the VM's current mutation boundaries.

`PhpArray` owns the packed/mixed storage boundary. The packed variant is used
for exact `0..len` integer-key arrays, including safe overwrites, append, and
tail `array_pop` updates. The storage converts to mixed for string keys,
non-sequential integer keys, holes from middle removals, or appends whose
auto-index no longer matches the current packed length. Mixed storage preserves
insertion order, overwrite position, next append-key behavior, and internal
pointer semantics. VM and JIT code must continue to consume
`PhpArray::packed_metadata()`, `shape_metadata()`, and guarded lookup helpers
instead of matching on storage variants.

The current packed variant still stores full key/value entries so
`PhpArray::iter()` can yield borrowed keys without changing callers. A
values-only packed buffer remains a deferred fast path behind the same facade.

String storage exposes `separate_for_write` and `bytes_mut`, but source-level
string-offset assignment is still a runtime known gap:
`E_PHP_RUNTIME_COW_STRING_OFFSET_WRITE`. Until the IR/VM has a string-offset
write instruction, Runtime semantics fixtures document that limitation explicitly.

## Reference Examples

```php
$a = 1;
$b =& $a;
$b = 2;      // $a and $b both read 2
unset($a);   // removes only the name $a
$b = 3;      // $b remains a live reference cell
```

```php
$a = 1;
$b = 2;
$c =& $a;
$c =& $b;    // $c is rebound to $b; $a remains 1
$c = 4;      // $b and $c read 4
```

Array-element references are executable for direct dimension lvalues.
Object-property references are still an explicit known gap:
`E_PHP_IR_UNSUPPORTED_PROPERTY_REFERENCE`.

## Public API Surface

Standard library reference and COW work should reuse:

- `php_runtime::Slot` for writable storage;
- `php_runtime::ReferenceCell` for alias identity;
- `php_runtime::TempValue` for non-referenceable VM temporaries;
- `php_runtime::PhpArray` and `PhpString` for shared payload storage and
  separation-on-write;
- `php_vm::frame::LocalFile` and VM lvalue helpers for local/global binding.

`ReferenceCell::try_get`, `ReferenceCell::try_set`, and
`ReferenceCell::try_with_value` are the preferred non-panicking inspection
helpers outside VM-internal paths that already control borrow ordering.

The architectural decision is recorded in
`docs/adr/0027-runtime-semantics-slot-reference-cow.md`.

## Risks and Optimization Points

Reference writes and array COW separation sit on hot VM paths. Standard library should
measure repeated append, nested dimension writes, by-reference parameter calls,
and foreach-by-reference loops before changing storage layout. Optimizations
must preserve the difference between by-value COW sharing and true
`ReferenceCell` alias identity.

The unsafe area is semantic rather than Rust `unsafe`: accidentally treating a
temporary as an lvalue can create write-through behavior PHP would not allow.
New lvalue kinds should be added through explicit slot/reference APIs and
covered by diff fixtures.
