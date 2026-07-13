# Cranelift Helper Symbol Registry

A later stage defines the stable helper-symbol registry used by later
Cranelift lowering work items. a later stage makes the checked integer add/mul
helpers executable from Cranelift-generated code. a later stage supersedes
the executable add/mul helper path for eligible int-only arithmetic with inline
checked Cranelift operations, while keeping these helper symbols available for
fallback and future non-inline paths.

## Registry

The registry lives in `crates/php_jit/src/helpers.rs` and exports:

- `JIT_HELPER_REGISTRY_ABI_HASH`
- `JIT_HELPER_SYMBOLS`
- `JitHelperId`
- `JitHelperArgKind`
- `JitHelperReturnKind`
- `JitHelperSymbol`
- `lookup_helper_by_id`
- `lookup_helper_by_name`

## Registry Symbols

| ID | Symbol | Purpose |
| --- | --- | --- |
| 1 | `phrust_jit_i64_add_checked` | Checked PHP integer addition helper, signature `(i64, i64, out_ptr) -> status`. |
| 2 | `phrust_jit_i64_mul_checked` | Checked PHP integer multiplication helper, signature `(i64, i64, out_ptr) -> status`. |
| 3 | `phrust_jit_strlen_known` | Known-shape `strlen` helper. |
| 4 | `phrust_jit_count_known` | Known-shape `count` helper. |
| 5 | `phrust_jit_string_concat` | VM-owned string concatenation helper. |
| 6 | `phrust_jit_packed_array_fetch` | Read-only packed-array integer-index fetch helper. |
| 7 | `phrust_jit_guard_failed` | Guard-failure side-exit helper. |
| 8 | `php_jit_array_is_packed_ints` | Conservative packed-int array layout guard. |
| 9 | `php_jit_array_len` | Read-only packed-array length helper. |
| 10 | `php_jit_array_fetch_int_slow` | Safe read-only packed-array integer fetch helper. |
| 11 | `php_jit_property_load_monomorphic_fast` | Guarded monomorphic property-load helper. |

## Stability Rules

- IDs are sorted and never reused.
- Symbol names are unique.
- Argument and return kinds are `repr(u32)`.
- Helper ids are `repr(transparent)` over `u32`.
- Any registry shape or meaning change updates `JIT_HELPER_REGISTRY_ABI_HASH`,
  tests, and this document in the same work item.

## Checked Integer Helper ABI

The add/mul helpers receive two `i64` operands plus an output pointer encoded in
the registry as `U64`, then return a `Status` code:

- `0`: result was written to the output pointer.
- `1`: the helper cannot preserve PHP semantics in native code, so the VM must
  fall back to the interpreter.

Overflow uses status `1`. The native Cranelift caller checks this status
immediately after every helper call and returns non-zero status through the
native entry ABI instead of using inline raw integer arithmetic.

A later stage adds status `2` for inline integer overflow exits. Eligible
int-only add/sub/mul no longer calls the add/mul helpers in native rows; it
branches on Cranelift overflow flags and returns status `2` to the VM when the
interpreter must resume. Helper-call rows and future helper-backed operations
still use the registry ABI above.

## Packed Array Helper ABI

A later stage adds the read-only packed-array helper symbols used by later
packed-array fast paths:

| ID | Symbol | Purpose |
| --- | --- | --- |
| 8 | `php_jit_array_is_packed_ints` | Conservative packed-int layout and alias guard. |
| 9 | `php_jit_array_len` | Packed-array length helper, returning status plus an out pointer. |
| 10 | `php_jit_array_fetch_int_slow` | Safe helper-assisted integer element fetch. |

The implementation lives in `php_runtime::jit_array` because runtime array
layout is owned by `php_runtime`. The helper registry records stable symbol
metadata only. These helpers are read-only, accept shared COW storage for
read-only fetches, reject reference elements, and require interpreter fallback
for non-packed arrays.

A later stage makes `php_jit_array_fetch_int_slow` executable from the
Cranelift packed-array int-index fetch path. Native code passes the runtime
array value pointer, the integer index, and an output pointer, then returns the
helper status through the normal native status ABI:

- `0`: integer element was written and the VM records
  `packed_fetch_fast_hits`;
- `2`: bounds side exit, recorded as `packed_fetch_bounds_exits`;
- `3`: layout side exit, recorded as `packed_fetch_layout_exits`.

Status `1` remains the generic fallback status for non-executable helper uses.

## Property Load Helper ABI

A later stage adds `php_jit_property_load_monomorphic_fast`, a VM-owned
helper for the narrow monomorphic property-load fast path. Native code passes
the runtime object value pointer, a pointer to handle-owned
`JitPropertyLoadMetadata`, and an output pointer. The helper returns status
through the normal native status ABI:

- `0`: cloned property value was written to the output pointer and the VM
  records `property_load_fast_hits`;
- `21`: receiver class guard failed;
- `22`: class-layout version guard failed before helper entry;
- `23`: typed property storage exists but is uninitialized;
- `24`: expected property storage was not present.

The helper fetches through the runtime object API and allocates the returned
`Value` for immediate VM ownership transfer. Generated code does not read
object storage directly, does not perform property writes, and does not compile
dynamic property loads.

## Validation

```bash
nix develop -c cargo test -p php_jit helper_registry
nix develop -c cargo test -p php_jit --features jit-cranelift helper_registry
```
