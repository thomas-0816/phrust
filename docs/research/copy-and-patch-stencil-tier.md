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
fixture that exercises the guarded property fetch/assign stencils) and produced:

| Metric | Value |
| --- | ---: |
| Dense instructions | 354 |
| Stencils | 285 |
| Patch sites | 428 |
| Helper calls | 32 |
| Live-state slots | 440 |
| Deopt points | 48 |
| Estimated code size | 3280 bytes |
| Compile-cost units | 454 |
| Work-to-compile ratio | 0.628 |

## Candidate Stencils

| Candidate | Current report shape | Required exits and metadata |
| --- | --- | --- |
| Load local | `load_local` over `load_local` and fused `load_local_echo`. | Frame-layout epoch, local-slot patch, destination register, source span. |
| Guard int/string/bool | Represented through guarded arithmetic, known builtin calls, and branch guards. | Exact value class feedback, source span, resume instruction, guard failure reason. |
| Int add/sub/mul with overflow exit | `guarded_int_arithmetic` over `binary_add`, `binary_sub`, and `binary_mul`. | Operand registers, destination register, overflow exit, type exit, checked helper/inline op identity. |
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
- string, comparison, division, modulo, bitwise, unary, and concat opcodes that
  still need PHP-semantic helper contracts or string/allocation state.

Object property fetch and assign are now supported candidates
(`guarded_property_fetch` / `guarded_property_assignment`) once dense bytecode
gained property opcodes; they still require the receiver class/layout-epoch
guards, visibility scope, magic/`__get`/hook rejection, and uninitialized
typed-property exits listed in the candidate table before an executable tier
can emit them.

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

The prototype makes stencil-library research concrete enough to compare against
the current Cranelift no-exec/reporting flow. It does not justify native
execution. Property dense opcodes now exist, so the property fetch/assign
stencil candidates are classified rather than reported absent. The next useful
work is the remaining prerequisite closure: typed live-state/deopt maps, helper
status contracts, frame identity maps, reference/COW materialization,
invalidation epochs, and executable-memory/W^X policy.

The PHP-aware mid-tier design in `docs/research/php-mid-tier-compiler.md` is the
next higher research layer. It consumes the same dense bytecode, feedback, and
deopt prerequisites, but targets guard sharing and PHP-semantic region planning
instead of very low-latency instruction templates.
