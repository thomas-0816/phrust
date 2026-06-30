# Runtime State Ownership

This document is the ownership contract for VM state, frame storage,
references, copy-on-write values, and request-global state. It describes the
rules that optimization work must preserve; PHP-visible behavior remains
defined by reference-PHP differential fixtures and known-gap entries.

## VM State

`ExecutionState` owns request-local dynamic runtime state for one VM execution:
global symbols, static locals, declaration/runtime registries, include/once
state, autoload state, suspended generator continuations, suspended fiber
continuations, destructor queues, and request-local builtin/runtime context.
No process-local cache may retain mutable request state after execution. Shared
engine data may cache compiled or resolved metadata only when invalidation is
based on source fingerprints, include/eval/autoload epochs, and declaration
table epochs.

`CallStack` owns the active `Frame` list plus a request-local frame pool. Active
frames are roots for locals, registers, argument snapshots, trace arguments,
`$this`/scope metadata, and call-site source spans. The frame pool is not a
root set: a frame must clear PHP-visible values and metadata before entering
the pool, and a reused frame must be reset to the exact function's register and
local counts before activation.

Generator and fiber continuations own their suspended frames. Suspension paths
move frames into continuation storage with `pop()`, not `pop_recycle()`.
Continuation frames must never be returned to the request-local frame pool while
suspended.

## Frames, Locals, and Registers

`Frame` is an activation record, not a persistent function object. Its
`function`, class context fields, call span, argument snapshots, register file,
local file, and reuse flag are all activation-scoped.

`LocalFile` stores PHP variable slots as `Slot`. Local slots are PHP-visible
storage: writing through a local reference must update the shared
`ReferenceCell`, while `unset()` removes only the local name unless an operation
explicitly writes through an lvalue. Invalid local access returns a typed
`VmError` with a stable diagnostic code and operation context.

`RegisterFile` stores `TempValue`, not referenceable PHP storage. A register
value is a temporary snapshot; storing a `Value::Reference` into a register must
deref to the cell's current effective value. Invalid register access returns a
typed `VmError` with a stable diagnostic code and operation context.

Frame reuse is valid only for activations proven not to retain external aliases
or continuation state. Closure captures, by-reference params/returns,
generator/fiber activations, include/eval frames, class-context frames,
shared top-level locals, try/finally bodies, and destructor-sensitive object
allocation frames must use fresh frames or a documented exact fallback.

## Runtime Storage

`Slot` is the runtime storage unit for variables, array elements, object
properties where represented as storage, static locals, and static properties.
`Slot::Value` owns an ordinary PHP value. `Slot::Reference` aliases a
`ReferenceCell`, and reads/writes dereference that cell.

`ReferenceCell` is the only shared mutable reference cell. It owns an
`Rc<RefCell<Value>>`, but callers must treat the cell as opaque storage and use
checked accessors outside low-level runtime internals. `ReferenceCell` identity
is internal debug/test metadata only; it is not a PHP-visible identity.

`TempValue` is private temporary storage. It cannot become a writable alias. Any
reference value assigned into a temporary is dereferenced immediately.

`PhpArray` owns opaque copy-on-write array storage. Cloning an array shares
storage until the next mutation. All array mutations must go through
`prepare_for_write`/`storage_mut_for` by way of public array mutation APIs so
copy-on-write separation, mutation epochs, reference-element metadata, and
fast-path guards remain coherent.

`ObjectRef` owns runtime object identity and property storage. Object clones
create a new identity with a shallow property copy; property values may still
contain references, arrays, and objects that preserve their own storage rules.
Object-property lvalues must write through `Value::Reference` cells when a
property has been converted to reference storage.

## Request Globals and GC Roots

Request-global state includes `$GLOBALS`, superglobals, static locals, class
static properties, include/once state, autoload state, resources, builtin
context, output buffers, destructor queues, and suspended continuations. These
are request roots and must be scanned or explicitly represented when reasoning
about reference/COW/GC behavior.

The GC/debug scanner is a test and diagnostics surface. It scans explicit VM
roots, frame locals/registers, globals, static locals, class-table values,
temporaries, destructor queues, and reserved generator/fiber stack categories.
Collection test hooks may clear only entities proven unrooted by that snapshot;
they must not model PHP-visible `unset()` or destructor order.

Optimization tiers may use frame, array, reference, and object metadata only as
guards. A guard failure must fall back or deopt to the canonical VM path without
changing output, diagnostics, side-effect order, alias identity, copy-on-write
separation, destructor timing, or exit status.
