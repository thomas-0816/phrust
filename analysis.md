# Verdict

The current profile and the source agree: **Phrust is no longer slow because of compilation. It is slow because the generated native code still executes through a defensive, generic Rust runtime interface.**

At the current default branch, the source-performance head is `2926f32b`; the following commit only refreshes the performance document. The clean result is 440.72 ms p50 versus 26.92 ms for PHP-FPM, with roughly 19× the CPU work and 4.7× the peak RSS. The warm diagnostic request performs zero compilation and crosses 1,323,262 runtime-helper boundaries.

The recent changes are real improvements:

* p50 fell from 1,075.7 ms to 440.72 ms;
* helper calls fell by about 40%;
* call-frame traffic fell by about 31%;
* stable-ID builtin dispatch removed more than 8 MB of call transport per request.

But that last substantial builtin change improved clean p50 by only 2.2%. This is direct evidence that further one-builtin-at-a-time work is no longer a credible route to PHP speed.

The required principle is:

> **Validate once when code or a deployment image is published; trust the validated representation for every warm execution until explicit invalidation.**

The production hot path should contain PHP-semantic checks, but it should not repeatedly contain engine-integrity checks, generic fallback preparation, telemetry branches, string lookups, ABI validation, callsite validation, bounds validation, or conversion through general Rust `Value` objects.

---

# Where Phrust still validates and prepares the same things repeatedly

## 1. The complete class table is revalidated for every request

`Vm::execute()` invokes `validate_native_class_table()` before entering native code. That validator traverses classes, parents, methods, abstract methods, interfaces, visibility, and final-method constraints. The compiled unit is immutable and has already passed compilation, so repeating this on every warm request is unnecessary.

**Remove it from request execution.** Store one of these in `CompiledUnit`:

```text
PreparedClassValidation::Valid
PreparedClassValidation::Invalid(immutable diagnostic)
```

The server should refuse or publish the unit once. A request should never walk the class hierarchy to reconfirm it.

The same rule applies to every IR, ABI, relocation, class-layout, helper-ID, and native-entry validation that is currently repeated after publication.

## 2. The root unit is effectively relinked into each request context

`install_root_dynamic_unit()` recomputes native-entry signature hashes, scans functions and classes, populates external-function and class maps, updates epochs, and installs dynamic metadata into every new `NativeExecutionContext`.

This should become an immutable, process-owned:

```text
DeploymentNativeImage
```

containing:

```text
validated class table
function and method IDs
native function cells
prepared call binders
prepared builtin entries
prepared property descriptors
prepared constants
source/diagnostic descriptors
prelinked base-unit symbol graph
```

A request should retain an `Arc<DeploymentNativeImage>` and allocate only PHP-visible mutable state.

Runtime `eval()`, conditional declarations, and dynamic includes can live in a small overlay. Their existence must not force the immutable WordPress base image to behave as mutable on every call.

## 3. `NativeExecutionContext::new()` still builds too much request infrastructure

The constructor:

* scans all functions to calculate call-argument capacity;
* conditionally clones and sorts the environment;
* allocates or initializes numerous maps, sets, vectors, caches, state registries, and symbol tables;
* reserves capacity for up to one million value refcounts and one million value views;
* starts every request with empty method PICs and resolved-entry caches.

Build a worker-owned reusable request-state pool:

```text
WorkerNativeRuntime
    ├── immutable DeploymentNativeImage
    ├── persistent callsite caches
    ├── persistent class/property metadata
    ├── reusable value arena chunks
    ├── reusable frame arena chunks
    └── NativeRequestState reset for each request
```

Reset lengths and generations, not allocations and maps.

---

# The largest hot-path safety tax: the runtime ABI

## 4. Every helper first recovers the context through TLS

Generated code does not pass a native request context to ordinary helpers. `activate_native_context()` installs a raw context pointer in thread-local state, and every helper reaches it through `with_native_context()` or `with_native_context_for()`.

That path performs:

* a TLS lookup;
* a null-pointer test;
* a closure call;
* a runtime `collect_counters` branch;
* optional helper entry/exit instrumentation.

This happens at up to 1.3 million helper boundaries per request.

Replace the current ABI with:

```rust
extern "C" fn helper(
    runtime: *mut NativeRequestFastState,
    ...
) -> NativeResult;
```

Every compiled function and fragment should receive the same stable `NativeRequestFastState*` and pass it to callees and helpers.

That removes `ACTIVE_NATIVE_CONTEXT` from the hot path entirely.

## 5. The helper ABI treats internally generated code as untrusted FFI

Common helpers use:

```text
out pointer
null validation
operation-number validation
status return
stack load of output
status branch
```

The unary helper, for example, attempts a fast operation, falls through to TLS context recovery, decodes the value, maps an integer opcode to an enum, constructs a `NativeOperationContext`, invokes the generic operation, re-encodes the result, validates the output pointer, and returns a status.

The production internal ABI should use a register-returned result:

```rust
#[repr(C)]
struct NativeResult {
    value: u64,
    status: u64,
}
```

On AMD64 System V and AArch64, a small two-word result can be returned without a caller stack output slot. The exact lowering should be verified for both targets, but the architectural objective is fixed:

```text
no per-call output pointer
no output null check
no stack output slot
no output reload
```

Use distinct typed helper entrypoints instead of:

```text
native_binary(opcode, ...)
native_cast(opcode, ...)
native_property_fetch(mode, ...)
```

For example:

```text
php_add_generic
php_concat_generic
php_array_fetch_quiet
php_array_fetch_warning
php_property_fetch_declared_slow
php_property_fetch_dynamic
```

The compiler selects the helper. The helper does not switch on an operation ID and then validate it.

## 6. Telemetry is disabled, but its branch remains everywhere

Clean timing disables counters, yet `with_native_context_for()` and call dispatchers still test `collect_counters` at each invocation. Similar branches surround builtin timing and per-helper attribution.

Use two runtime-helper tables:

```text
production helper table
diagnostic helper table
```

The production functions contain no telemetry branch, no helper-name lookup, and no timer code.

This should be a compile-time or helper-table distinction, not a boolean checked over one million times.

---

# The current “direct” builtin path is still not direct

The stable-ID builtin work removed the large generic argument frame, but the builtin dispatcher still:

1. checks all pointers;
2. reconstructs slices;
3. looks up the callsite descriptor by function and continuation;
4. validates that the descriptor's helper ID matches the passed helper ID;
5. retrieves a semantic instruction;
6. binds arguments using the builtin's **name**;
7. calls `execute_native_builtin()` using that name.

That explains why eliminating 18.7% of call transport improved p50 by only 2.2%: the generic semantic path remained.

A direct builtin call must instead be:

```text
generated code
    → exact prepared handler/function pointer
    → exact capability state
    → result
```

No name, registry lookup, descriptor lookup, helper-ID check, `BuiltinContext`, or generic binder.

Each published callsite should hold a validated immutable record:

```rust
PreparedBuiltinCall {
    handler: NativeBuiltinHandler,
    capability: BuiltinCapability,
    argument_plan: FixedArgumentPlan,
    diagnostic: DiagnosticSiteId,
}
```

Pure builtins should receive no request capability state. Session state should be supplied only to session builtins, filesystem state only to filesystem builtins, and so on.

---

# The generic call binder remains a major execution engine

The native function binder still performs substantial work per call:

* retrieves prepared metadata;
* checks receiver/capture counts;
* duplicates leading arguments;
* discovers variadics;
* determines whether arguments are positional;
* allocates assigned/default/variadic structures;
* resolves names case-insensitively;
* handles unpack;
* decodes and re-encodes values;
* checks by-reference requirements;
* performs scalar coercions and type checks;
* constructs visible trace arguments.

The report still records 233,308 runtime-mediated callsites and 35.36 MB of call transport. Nearly 97,000 calls remain classified as dynamic, many for reasons such as omitted defaults, by-reference metadata, unpublished targets, or extra positional arguments—conditions that can usually be compiled into a stable binder rather than resolved repeatedly.

Build one immutable binder plan per `(callsite, target signature)`:

```text
source operand → destination parameter mapping
default template IDs
by-reference mask
type-check/coercion plan
variadic packing rule
extra-argument rule
trace metadata
result ownership
```

The generated callsite executes the binder plan directly.

Examples:

* An omitted required argument should call a prepared throw stub, not enter dynamic resolution.
* An omitted array default should clone a prepared default template.
* Extra positional arguments should be placed into the prepared extra-argument area.
* A by-reference callsite should use a precomputed lvalue plan.
* Typed parameters should use generated guards/coercions, not the general binder.

The generic binder remains only for targets that are genuinely unknown at runtime.

---

# Request-local inline caches are throwing away useful work

`NativeExecutionContext` creates a new method-PIC map and a new resolved-entry cache for every request. The PIC uses a `BTreeMap`; entries contain `Arc<str>` receiver-class and method names plus epochs. The resolved-entry table also checks a signature epoch on every hit.

Move these to engine-owned callsite metadata:

```text
CallsiteId → atomic/PIC state
```

A monomorphic method call should be:

```text
load receiver class ID
compare immediate class ID
load target cell
native call
```

Not:

```text
BTreeMap lookup
Arc string comparison
method-name comparison
epoch comparison
runtime dispatch
```

In immutable deployment mode, base class and function tables do not need a signature epoch test on every call. Invalidation should atomically clear or replace the target cell.

---

# The native value arena is still a general object store

The profile records:

* 484,557 value-table allocations;
* 245,687 releases;
* a 251,855-slot high-water mark;
* 96,342 releases reaching zero.

The implementation explains the traffic:

* `decode()` clones the Rust `Value`;
* `encode()` hashes object/reference identities;
* it validates free slots;
* maintains `values`, `value_refcounts`, and `value_views` as parallel vectors;
* retains interned handles;
* checks refcount overflow;
* records free slots and removes identities;
* a zero-reaching object may trigger destructor/root handling.

This representation must be replaced, not further patched.

## Native value model required

Use a stable, compact native slot:

```rust
#[repr(C)]
struct NativeValueSlot {
    tag: u32,
    flags: u32,
    payload: u64,
    aux: u64,
}
```

The payload may represent:

```text
immediate null/bool/int
unboxed or boxed float
stable string pointer/handle
array header pointer
object pointer
reference-cell pointer
resource/callable/generator/fiber handle
```

The compiler and helpers operate on borrowed slot views. A Rust `Value` is materialized only when an extension API or genuinely generic semantic operation requires it.

## Lifetime policy required

Not every temporary needs immediate reference counting.

Use:

```text
compiler last-use tracking
request arena for ordinary temporaries
bulk request reset
explicit COW ownership for arrays
explicit reference-cell ownership
explicit resource/destructor lifetime
```

Ordinary strings, arrays without observable destruction, and transient wrapper values should not require a separate arena entry plus a release helper merely because one operation produced them.

The final target should be:

```text
value-table allocations below 50,000/request
release helpers below 25,000/request
high-water mark below 30,000 slots
```

---

# Arrays, foreach, and properties must become a native data plane

The largest helper family is arrays and foreach at 531,418 calls. Individual counts include 241,491 array fetches, 118,573 inserts, and 98,500 foreach-next operations. Properties add another 109,262 calls.

These operations cannot remain Rust helper calls if the target is 27 ms.

## Required stable array ABI

Expose a small validated array header to generated code:

```text
storage kind
length
capacity
COW/shared flag or count
shape ID
data pointer
version
```

Cranelift should directly implement:

* packed-array length;
* packed integer-index fetch;
* packed iteration;
* `isset`/null-coalesce;
* unique packed append;
* stable record-shape key lookup;
* stable record iteration.

Use typed slow helpers for:

* key coercion;
* hash resize;
* COW separation;
* references;
* magic/user callbacks;
* uncommon mixed layouts.

A `foreach` over an ordinary packed or stable record array should keep its cursor in the native frame and execute no helper on every iteration.

## Required property ABI

Prepared property descriptors should contain:

```text
expected class/layout ID
declared slot index
initialization/type flags
magic/hook slow-path ID
```

A declared property read should be a class/layout guard followed by a direct slot load. Only uninitialized, hooked, magic, dynamic, inaccessible, or layout-mismatch cases should call a runtime helper.

---

# Remove local fallbacks from optimized code

Recent fast-path lowering still creates a `fast`, `slow`, and `merge` block at each suitable string or array builtin callsite, and the `slow` block embeds the complete generic native call trampoline.

That is safe, but it duplicates fallback machinery throughout native code, increases code size, worsens instruction-cache pressure, and keeps the generic runtime path tightly coupled to every optimized operation.

Use version-level fallback:

```text
baseline native fragment:
    complete generic semantics

optimized native fragment:
    entry/loop shape guards
    compact direct operations
    native transition to baseline continuation on guard failure
```

Do not embed a full generic fallback under every operation.

For operation-specific rare cases, call one shared out-of-line cold stub. Cranelift already has a function-local terminal-exit mechanism; extend that idea to shared semantic slow stubs rather than duplicating the entire trampoline at each callsite.

The optimized fragment should contain no fallback code that is not exercised by its admitted profile.

---

# ABI and entry validation must happen once

`JitFunctionHandle` checks ABI hash and exact arity whenever Rust invokes a native entry. Native unwind and suspension paths repeat those validations and search transition metadata.

Create two types:

```rust
UnvalidatedNativeEntry
TrustedNativeEntry
```

Artifact loading and publication convert the first to the second after checking:

```text
ABI
signature
generation
target
metadata
relocations
```

Warm calls use `TrustedNativeEntry::invoke_unchecked()` internally. Public/debug APIs may retain the checked path.

This is also how helper IDs and callsite descriptors should work: one validation when native code is linked, no repeated validation while executing it.

---

# One production architecture, two validation modes

Do not simply delete cold safety checks. Move them.

## Retain outside the warm path

Keep:

* native artifact checksum validation;
* relocation validation;
* helper-ID and ABI validation at load/publication;
* W^X enforcement;
* compile-complexity limits;
* source/IR validation;
* PHP-visible type, reference, COW, visibility, exception, and warning semantics.

Warm compilation is already zero, so removing cache validation will not improve the current 440 ms request.

## Remove from production execution

Remove:

* TLS context recovery per helper;
* pointer/null checks for internally generated call frames and output slots;
* ABI/hash/arity checks on prevalidated entries;
* helper-ID validation on published direct builtin sites;
* callsite descriptor lookup on direct calls;
* operation-ID switches for fixed helper operations;
* runtime telemetry branches;
* per-request class-table validation;
* per-request symbol/signature relinking;
* request-local method/function IC rebuilding;
* generic argument binding for stable callsites;
* `Value` clone/decode/encode for common operations;
* local generic fallbacks inside optimized fragments.

A diagnostic or hardened helper table may preserve these assertions for tests. It should not be the production table.

---

# The three fundamental infrastructure changes

## A. Sealed deployment image and trusted ABI

Build once:

```text
validated class/function graph
numeric class/function/method IDs
prelinked callsite records
binder plans
property descriptors
builtin handler pointers
constant/default templates
native entry cells
```

Pass a direct `NativeRequestFastState*` through all native calls and helpers. Compile telemetry and defensive checks out of the production helper table.

This removes repeated engine-integrity work.

## B. Native value, array, property, and lifetime plane

Replace the general Rust `Value` arena boundary with:

```text
stable native slots
borrowed native views
request arena allocation
compiler last-use ownership
direct packed/record arrays
direct foreach loops
direct declared-property slots
```

This removes most of the 1.3 million helper boundaries and almost half a million value allocations.

## C. Fully linked native call plane

Use:

```text
atomic function cells
worker-persistent callsite PICs
precompiled binder plans
direct capability-specific builtins
compiled-to-compiled cross-unit calls
native result destinations
```

Only genuinely dynamic callables enter the general resolver.

This removes repeated call validation, frame transport, argument binding, and runtime dispatch.

These three changes should be developed as coordinated architecture work. They are not a list of experiments.

---

# Hard acceptance contract

A serious integration tranche should not be accepted merely because it improves p50 by another 2–5%.

Require simultaneous movement in all of these:

| Metric                              |                Current | Required first architecture gate |
| ----------------------------------- | ---------------------: | -------------------------------: |
| Warm c1 p50                         |              440.72 ms |                      **≤ 80 ms** |
| Final parity target                 |               26.92 ms |         **≤ 40 ms, then parity** |
| Helper boundaries                   |              1,323,262 |                    **≤ 150,000** |
| Value-table allocations             |                484,557 |                     **≤ 50,000** |
| Value releases                      |                245,687 |                     **≤ 25,000** |
| Value-table high-water              |                251,855 |                     **≤ 30,000** |
| Dynamic calls                       |                 96,915 |                     **≤ 10,000** |
| Stable direct-call rate             | ~58% of mediated sites |                        **≥ 95%** |
| Native call transport               |               35.36 MB |                       **≤ 3 MB** |
| Array/foreach/property helper calls |               ~640,000 |                     **≤ 50,000** |
| Per-request class validation        |                present |                         **zero** |
| Production telemetry branches       |                present |                         **zero** |
| Peak RSS                            |               646.4 MB |                     **≤ 250 MB** |

A change should not merge as a performance tranche unless it removes a shared cost block and improves wall time, helper count, value traffic, call traffic, and RSS together.

# Bottom line

Phrust has spent the recent commits making a generic, defensive runtime ABI cheaper. That work has produced genuine gains, but the 16× gap proves that the ABI itself now has to go.

The next engine must operate under this warm-path invariant:

```text
validated once
numeric IDs only
no TLS context lookup
no generic Value conversion
no generic binder for stable calls
no repeated ABI/callsite validation
no telemetry branch
no local fallback machinery
direct native data operations
direct native calls
```

Removing isolated pointer checks or one more builtin frame will not close the gap. **Removing the general runtime boundary from ordinary PHP execution can.**

