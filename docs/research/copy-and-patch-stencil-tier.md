# Copy-And-Patch Stencil Tier Research

Date: 2026-06-28.

Reference target: PHP 8.5.7 (`php-8.5.7`).

This document defines a no-exec copy-and-patch stencil prototype for dense VM
bytecode. It is research and planning evidence only: the prototype does not
allocate executable memory, does not emit machine code, does not install a
runtime mode, and does not run native code.

## Why This Tier Exists

Cranelift is valuable for a small set of proven numeric and packed-array
kernels, but its setup cost can be too high for warm PHP request traces where
the region is short, guard-heavy, and helper-heavy. A copy-and-patch tier could
compile faster by selecting predesigned instruction templates, patching frame
slots, register indexes, branch targets, helper IDs, and side-exit targets, then
falling back to the interpreter for any semantic ambiguity.

That lower compile latency is only useful if the template library consumes the
same dense bytecode, quickening metadata, inline-cache state, deopt maps, and
helper ABI as the interpreter and Cranelift paths. It must not introduce a
second parser, AST, semantic frontend, bytecode format, or source-string
execution path.

## Stencil Format

The report-only format is intentionally textual and platform-neutral:

```text
stencil {
  instruction_kind: guarded_int_arithmetic
  dense_opcode: binary_add
  patch_sites: lhs_register, rhs_register, destination_register, overflow_exit
  guard_dependencies: lhs_is_int, rhs_is_int
  helper_calls: phrust_jit_i64_add_checked
  live_state_requirements: operand_registers, destination_register,
    source_span, resume_instruction
  side_exit_target: interpreter_overflow_or_type_exit
}
```

The JSON command uses the same fields for each stencil:

- `instruction_kind`;
- `patch_sites`;
- `guard_dependencies`;
- `helper_calls`;
- `live_state_requirements`;
- `side_exit_target`;
- `code_size_bytes_estimate`;
- `compile_cost_units`.

The top-level report includes `native_execution: false`,
`executable_memory: false`, estimated code size, patch-site counts,
helper-call counts, unsupported reasons, deopt point counts, live-state slot
counts, and a `work_to_compile_ratio`.

## Current Generator

The report generator is:

```bash
php-vm dump-copy-patch-stencils <file.php> --json
```

The smoke gate is:

```bash
nix develop -c just copy-patch-stencil-smoke
```

It writes per-fixture reports and an aggregate summary under:

```text
target/performance/stencils/
```

The generator compiles through the normal frontend, lowers verified IR to dense
bytecode, applies conservative superinstruction selection metadata, verifies
dense bytecode, then maps a tiny quickening-compatible dense subset to
stencils. It never allocates executable memory.

The current smoke summary covers five fixtures (including the `properties.php`
fixture that exercises the guarded property fetch/assign stencils, and
`arithmetic.php` whose loop condition exercises `guarded_int_comparison`) and
produced:

| Metric | Value |
| --- | ---: |
| Dense instructions | 354 |
| Stencils | 290 |
| Patch sites | 448 |
| Helper calls | 32 |
| Live-state slots | 460 |
| Deopt points | 53 |
| Estimated code size | 3400 bytes |
| Compile-cost units | 464 |
| Work-to-compile ratio | 0.625 |

## Candidate Stencils

| Candidate | Current report shape | Required exits and metadata |
| --- | --- | --- |
| Load local | `load_local` over `load_local` and fused `load_local_echo`. | Frame-layout epoch, local-slot patch, destination register, source span. |
| Guard int/string/bool | Represented through guarded arithmetic, known builtin calls, and branch guards. | Exact value class feedback, source span, resume instruction, guard failure reason. |
| Int add/sub/mul with overflow exit | `guarded_int_arithmetic` over `binary_add`, `binary_sub`, and `binary_mul`. | Operand registers, destination register, overflow exit, type exit, checked helper/inline op identity. |
| Int comparison with type exit | `guarded_int_comparison` over the dense compare opcodes (`compare_equal`/`compare_not_equal`/`compare_identical`/`compare_not_identical`/`compare_less`/`compare_less_equal`/`compare_greater`/`compare_greater_equal`/`compare_spaceship`). Native integer compare once both operands are proven int. | Operand registers, destination register, `lhs_is_int`/`rhs_is_int` guards, type exit to the interpreter comparison ladder, resume instruction. |
| Int bitwise with type exit | `guarded_int_bitwise` over `binary_bit_and`/`binary_bit_or`/`binary_bit_xor`. Single native op on two proven ints; the string form and coercion cases side-exit. | Operand registers, destination register, `lhs_is_int`/`rhs_is_int` guards, type exit to the interpreter bitwise ladder, resume instruction. |
| Int shift with range exit | `guarded_int_shift` over `binary_shift_left`/`binary_shift_right`. Native shift on proven ints, but PHP throws on a negative shift amount and defines out-of-range shifts. | Operand registers, destination register, `lhs_is_int`/`rhs_is_int` guards, `shift_amount_in_range` guard, negative/out-of-range shift and type exit, resume instruction. |
| Packed array guard/fetch | `packed_array_guard_fetch` over dense `fetch_dim`. | Packed-array guard, integer key guard, OOB exit, warning/diagnostic ordering, no by-reference element state. |
| Object shape guard/property slot load | `guarded_property_fetch` over dense `fetch_property`, and `guarded_property_assignment` over dense `assign_property`. Dense bytecode now exposes property fetch/assign opcodes, so the stencil tier classifies them instead of reporting `object_shape_property_load_dense_opcode_absent`. | Receiver class/layout epoch, property slot, visibility scope, magic/get/hook rejection, uninitialized typed-property exit. |
| Known builtin call | `known_builtin_call` for dense direct calls to `strlen` and `count`. | Function-table epoch, builtin identity, argument shape, helper return-status exit, diagnostics order. |
| Branch guard | `branch_guard` over conditional dense jumps. | Boolean condition guard, branch-bias metadata, taken/fallthrough patch sites, resume block. |
| Return | `return` over dense `return`. | Return value, caller frame, destructor order, slow return exit. |
| Side exit to interpreter | Each guarded stencil records an interpreter side-exit target. | Exact resume instruction, live locals/registers/temporaries, source span, pending diagnostics/output state. |

## Unsupported Shapes

The prototype rejects or records gaps for:

- array creation, insertion, append, and assignment, because mutation needs
  reference/COW identity, allocator state, and diagnostics order;
- dynamic or userland calls, because call frames, symbol state, autoload, and
  by-reference arguments need exact live-state maps;
- foreach, because iterator position, mutation epoch, by-reference mode, and
  resume state are not native-representable yet;
- string, division, modulo, power, unary, and concat opcodes that still need
  PHP-semantic helper contracts or string/allocation state (integer bitwise and
  shift opcodes now have guarded stencils; see the candidate table).

Object property fetch and assign are now supported candidates
(`guarded_property_fetch` / `guarded_property_assignment`) once dense bytecode
gained property opcodes; they still require the receiver class/layout-epoch
guards, visibility scope, magic/`__get`/hook rejection, and uninitialized
typed-property exits listed in the candidate table before an executable tier
can emit them. Integer comparison is likewise a supported candidate
(`guarded_int_comparison`): the compare opcodes leave the generic
PHP-semantic-helper bucket and gain `lhs_is_int`/`rhs_is_int` guards with a type
exit to the interpreter comparison ladder for any non-int operand.

## Required Deopt Metadata

Before any executable stencil tier can exist, every patch point and helper call
needs:

- dense function, block, and instruction indexes;
- source span and source-map target;
- VM register/local live-state map;
- side-exit reason and interpreter resume target;
- helper ABI hash and helper status contract;
- frame identity and caller/callee return slot;
- reference/COW/alias state for values reachable from the stencil;
- diagnostic, output buffering, exception, destructor, generator, and fiber
  state rejection or materialization rules;
- invalidation epochs for functions, classes, properties, includes, autoload,
  array/object shape metadata, and persistent feedback.

## Frame-Local Slot ABI

How a native stencil reads and writes PHP locals given only the opaque frame
handle. This is the contract the next executable increment consumes.

**Constraints from the current model.** The VM frame stores locals and
registers as `LocalFile`/`RegisterFile` of VM `Value`/reference slots — *not* a
flat native array. `JitCFrameView` (`crates/php_jit/src/abi.rs`) carries only an
opaque `frame` handle plus `local_count`/`register_count`; it never exposes a
raw locals pointer. The region IR already names the working set:
`RegionOsrEntry.live_slots: Vec<VmSlotId>` (`region_ir/osr.rs`),
`RegionNodeKind::Param { slot }` reads a live slot, and `VmSlotKind`
(`region_ir/bind.rs`) distinguishes `local`/`register`/`temporary`/… so the
same descriptor addresses both files.

**Model: marshal-in → native slot buffer → marshal-out.**

1. **Marshal-in (Rust, before the call).** The VM allocates a flat slot buffer
   of `JitCValue` (24 bytes, `repr(C)`), one entry per `VmSlotId` in
   `live_slots`, densely renumbered to a native slot index. It converts each
   live VM slot `Value` → `JitCValue` and writes `buffer[native_index]`, then
   passes `slot_base` to the region alongside the `JitCFrameView`.
2. **Native fast path (no per-access helper).** Native code addresses slot `i`
   at `slot_base + i*24` directly. Because `JitCValue` is exactly 24 bytes,
   every field lands on a legal scaled-immediate boundary: `tag` at
   `i*24 + 0` via `ldr_w`/`str_w` (÷4), `payload` at `i*24 + 8` and `aux` at
   `i*24 + 16` via `ldr_x`/`str_x` (÷8). The existing emitter offset load/store
   in `aarch64.rs` already encode these; **no new instruction is required.**
   `Param{slot}` lowers to a load; a slot write lowers to a store.
3. **Marshal-out (Rust, on normal return *and* every side exit/deopt).** The VM
   reads `buffer[native_index]`, converts `JitCValue` → VM `Value`, and commits
   it back to the frame slot using the *same* `live_slots` → native-index map,
   so the interpreter always resumes with a consistent frame.

Because the VM holds the frame and invokes the region from Rust, all marshaling
is pure Rust around the `blr`; frame-local access therefore needs **no new
`extern "C"` helper** — native code only ever touches the caller-provided
buffer. Helpers stay reserved for operations that genuinely need VM-owned state
(allocation, slow-path property/array access), consuming opaque handles only.

**Bail-before-entry (never marshal in a shape the subset cannot execute),**
mirroring the OSR motion policy in `region_ir/osr.rs`:

- any live slot holding a reference cell, COW-shared array/object, or otherwise
  `reference_or_cow_state` → reject the region (the interpreter runs it);
- non-scalar / unrepresentable value kinds in the working set → reject.

This keeps the buffer a flat, owned, scalar-only working set for the guarded
subset; heap/opaque values stay VM-owned and cross the ABI only as opaque
handles a helper explicitly consumes.

**Addressing limit.** The scaled-immediate field is 12 bits (`imm12` ≤ 4095), so
`ldr_x`/`str_x` reach byte offset ≤ 32760 → native slot index ≤ 1365 directly.
Functions whose live set would exceed that are rejected as an unsupported shape
until base-adjust addressing (add the slot base into a scratch register first)
is emitted.

## Executable Prerequisites (report-only)

These close non-execution prerequisites. None of them allocate executable
memory or run native code; they define the contracts a future tier would need
and are surfaced in the stencil report so drift is visible.

### Helper ABI / status contract

Stencils call VM-owned helper functions (e.g. `php_jit_property_fetch_slow`).
A stable `helper_abi_hash` is computed over the sorted set of distinct helper
symbols the report's stencils reference. It changes whenever the helper set
changes, so a future code cache can reject stencils compiled against a stale
helper ABI. The hash is report-only metadata today.

### Code-cache key schema

A future stencil code cache must be keyed so a stale or mismatched artifact is
never reused. The schema is:

- dense bytecode version;
- function ID and IR fingerprint;
- helper ABI hash (above);
- target architecture / config label;
- feature flags and tier configuration;
- invalidation epochs (function, class, property, include, autoload, shape,
  persistent feedback).

The stencil report emits these fields so the key is defined and observable
before any cache exists.

### W^X / executable-memory policy

The tier is **no-exec by construction**. Policy:

- no page is ever mapped both writable and executable (W^X);
- a future emitter would write stencil bytes to a writable, non-executable
  buffer, then remap read-execute before any call, never both at once;
- the current generator allocates **no** executable memory and performs **no**
  `mmap`/`mprotect` with `PROT_EXEC`; `native_execution` and `executable_memory`
  are hard-coded `false` in every report.

### Verifier rule

The report carries `native_execution: false` and `executable_memory: false`.
A verifier/test asserts both remain `false`; if either becomes `true` without
the full W^X, code-cache, deopt, and PHPT/reference prerequisites, the gate
fails. This is the guard that keeps stencil research non-executable.

## Placement Against Existing Tiers

| Situation | Preferred tier |
| --- | --- |
| Cold code, unsupported semantic state, dynamic calls, mutation-heavy paths, or weak feedback. | Interpreter with counters. |
| Short warm regions dominated by simple locals, int arithmetic, branch guards, known builtins, or packed fetches. | Future copy-and-patch stencils, after executable-memory and deopt prerequisites are owned. |
| Hot regions where quickening, inline caches, and superinstructions already preserve behavior inside the VM. | Keep Tier 1 in the interpreter. |
| Warm regions that need shared shape guards, numeric-string specialization, branch layout, and exact PHP live-state metadata. | Future PHP-aware mid-tier, still report-only until prerequisites are closed. |
| Stable packed/numeric kernels that justify higher compile cost and pass Cranelift eligibility. | Default-off Cranelift packed/numeric region. |

## Architecture Decision

The `dump-baseline-native-stencil` **report command** stays no-exec: it
estimates and classifies, it does not emit machine code. Property dense opcodes
now exist, so the property fetch/assign stencil candidates are classified rather
than reported absent.

Separately, the executable prerequisites this doc listed as future work are now
owned and tested — executable-memory/W^X policy (`crates/php_jit/src/code_memory.rs`),
helper status contracts (`helpers.rs`), the live-state map (`region_ir/osr.rs`),
and typed side-exit/deopt reasons (`abi.rs`). A default-off aarch64 emitter
(`aarch64.rs`) executes scalar arithmetic/branch stencils over the `JitCValue`
ABI in tests using the Frame-Local Slot ABI above. Under ADR 0787 that guarded
subset is now `SUBSET_ALLOWED`. The remaining closure is frame-model completion,
code-cache lifecycle, source maps, reference/COW/foreach/exception
materialization for shapes beyond the scalar subset, and PHPT/reference parity —
all still required before broad generic execution or any default-on discussion.

The PHP-aware mid-tier design in `docs/research/php-mid-tier-compiler.md` is the
next higher research layer. It consumes the same dense bytecode, feedback, and
deopt prerequisites, but targets guard sharing and PHP-semantic region planning
instead of very low-latency instruction templates.
