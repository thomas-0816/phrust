# Deopt, Live-State, and OSR Metadata Foundation

Date: 2026-06-28.

This document records the FPE-16 metadata foundation for optimized exits. It is
report-only: it does not enable OSR, trace JIT execution, broad Cranelift
execution, or a baseline-native runtime switch.

## VM-Owned Schema

`php_vm::deopt` owns the shared metadata model future optimized tiers must
consume before they can resume in the interpreter from a non-entry point.

The schema includes:

- `VmDeoptReason`, with reason codes 1 through 7 matching the existing
  Cranelift side-exit codes: `type_mismatch`, `overflow`,
  `unsupported_value`, `guard_failed`, `helper_status`, `exception_pending`,
  and `abi_mismatch`.
- Additional VM-owned reasons for `call_frame_boundary`,
  `reference_cow_identity`, `foreach_iterator_state`, `pending_finally`,
  `generator_or_fiber_state`, `output_buffer_state`, and
  `unsupported_control_flow`.
- `DeoptResumePoint`, identifying dense function, block, and instruction.
- `LiveStateSnapshot`, containing register slots, local slots, an explicit
  empty operand stack for the current register VM, source span, pending
  exception/finally markers, foreach iterator marker, reference/COW marker,
  output-buffer marker, call-frame identity marker, and an optional
  caller-callee return slot for value-returning call boundaries.
- `DeoptRegionMetadata`, using dense basic blocks as conservative optimized
  regions for this foundation.

The metadata generator consumes rich IR first to reject state that cannot yet be
represented, then lowers to verified dense bytecode and emits side-exit points
for supported dense instructions.

## Supported Metadata Regions

The current generator covers verified dense bytecode regions for:

- straight-line scalar code;
- branches and conditional resume points;
- loops represented in dense control flow;
- by-value `foreach`, with explicit foreach iterator state markers;
- scalar arithmetic, comparison, unary, echo/output, call, array, and dimension
  instructions as metadata-only side-exit sites.

Generated metadata records possible side-exit reasons but does not compile or
execute native code. The interpreter remains the only executable path.

## Rejections

The generator rejects rich IR before dense lowering when state would require a
resume guess:

- `try`/`finally` and unwind state: `pending_finally`;
- explicit throw/exception construction paths: `exception_pending`;
- generators and future fiber-like suspension state:
  `generator_or_fiber_state`;
- local references, by-reference foreach, by-reference calls, and by-reference
  returns: `reference_cow_identity`;
- include/eval/runtime-gap control flow: `unsupported_control_flow`.

These rejections are intentional. A future optimized tier must add exact
representation and focused fixtures before any rejected shape can execute in an
optimized region.

## Verifier Rules

`DeoptMetadata::verify` enforces the report-only contract:

- metadata dense bytecode version must match the current dense bytecode version;
- `native_execution` must remain false;
- every region must contain at least one block and instruction;
- every side exit must resume inside its owning region and function;
- every snapshot resume point must match the side-exit region;
- operand-stack slots are rejected because the current VM is register-based;
- alias metadata must stay consistent (`alias_metadata_consistent`): a snapshot
  reporting no reference/COW control state may not carry a reference-sensitive
  slot;
- a slot marked uninitialized (`initialized == Some(false)`) may not appear in a
  snapshot that also claims full materialization — the initialized-state family
  must reject it;
- a recorded caller-callee return slot is valid only alongside a represented
  call frame and must index a real register slot.

In debug builds the generator additionally re-derives the initialized-liveness
claims from scratch (`debug_assert_initialized_liveness_sound`): for every
generated snapshot it recomputes, from the owning block's start, the slots
provably initialized before the resume instruction and asserts each
`Some(true)` claim is backed by an in-block definition. This independent
recomputation fails loudly in tests on any over-claim.

Dense bytecode structural validation remains owned by `DenseBytecodeUnit::verify`.

## Live-State Precision

The generator proves as much per-slot state as a single dense block can
establish, and leaves everything else unproven rather than guessing:

- **Initialized liveness.** `LiveValueSlot.initialized` is `Some(true)` for a
  register or local that is provably defined before the resume instruction on
  every path reaching it. This is proved intra-block: a dense block is
  single-entry and straight-line (only its terminator branches), so any path
  reaching instruction `i` executes `[block_start, i)` in program order, and a
  slot defined earlier in the same block is therefore initialized at `i`.
  `UnsetLocal` is the only opcode that can make a local uninitialized again and
  is tracked as a kill; register definitions are enumerated through the same
  exhaustive classifier the last-use pass uses (`collect_defs_uses`), so a new
  operand shape must be classified before it compiles. Slots first defined in a
  predecessor block stay `None` (unproven). The generator never emits
  `Some(false)` for the enumerate-all model — a not-yet-defined slot is
  "unproven", not "proven uninitialized" — so no currently-materializable
  snapshot is newly rejected.
- **Alias-class refinement.** A register whose latest in-block definition is a
  provably scalar-producing opcode (comparison, `instanceof`, boolean negation,
  `isset`/`empty`) can never carry reference or COW identity, so it is refined to
  the finest `no_references_observed` class even inside a reference/COW region
  where the region default is the coarse `unknown_aliasing`. The
  coarsest-class summary (`reference_alias_summary`) is unaffected, so the region
  stays honestly `unknown_aliasing` overall.
- **Caller-callee return slot.** At a `call_frame_boundary` side exit whose dense
  opcode names an explicit value destination (`CallFunction`, `CallCallable`,
  `NewObject`, `CallMethod`, `CallStaticMethod`, `Pipe`), the snapshot records
  the caller register that receives the callee return value.

`DeoptMetadata::precision_counters` reports these gains so they are measurable:
`slots_initialized_known` (slots proven `Some(true)`), `slots_alias_refined`
(register slots refined to no-reference inside a reference/COW region), and
`snapshot_rejected_by_family` (side-exit snapshots that cannot be materialized
exactly, keyed by the first missing state family — empty for the current
supported regions, since rejections happen upstream at the IR boundary).

## Validation Fixtures

The `php_vm` unit fixtures cover:

- straight-line scalar metadata generation;
- branch resume points;
- loop metadata;
- by-value foreach state representation;
- `try`/`finally` rejection;
- exception-path rejection;
- generator/fiber-state rejection;
- reference/COW rejection;
- include/eval hard rejection and by-reference foreach rejection;
- initialized-after-definition proof (`Some(true)`) and no over-claim across a
  conditional definition (`None` at the merge point);
- per-slot scalar alias refinement inside a reference/COW region, with an
  unchanged coarsest-class summary;
- caller-callee return-slot recording at a call boundary;
- verifier rejection of an inconsistent alias claim and of a return slot without
  a represented call frame or outside the register file;
- precision counters (`slots_initialized_known`, `slots_alias_refined`,
  `snapshot_rejected_by_family`);
- compatibility of VM deopt reason codes with the existing Cranelift side-exit
  prefix.

## Remaining Boundaries

Gate class: `HARD_BLOCK` (see `docs/performance/optimization-gates.md`).
This foundation is not a complete OSR implementation. The following remain
blocked until future focused work:

- native frame reconstruction from arbitrary instruction points;
- cross-block/parameter-entry initialized liveness (only intra-block
  definite-initialization is proved today; merge points and function parameters
  stay `None`);
- exact runtime reference alias classes and array element identity maps (the
  scalar-register refinement above is static and conservative; distinguishing
  local-only vs escaped vs global vs property/array-dim needs runtime values);
- pending exception object materialization;
- finally/destructor ordering across optimized exits;
- generator/fiber suspension snapshots;
- output-buffer callback state;
- cross-region trace linking and invalidation.

FPE-28 region profiles consume current VM counters and IR/source-map metadata
to classify future candidate regions, but they do not add the missing resume
state above. Their exception, `try`/`finally`, generator, and fiber entries are
rejection metadata, not executable deopt support.

## Alias-Class Markers

Each `LiveValueSlot` now carries an `alias_class` field aligned with the VM's
reference-aliasing model (`AliasState`): `no_references_observed`,
`local_only_reference`, `escaped_reference`, `global_or_superglobal_reference`,
`property_or_array_dim_reference`, or `unknown_aliasing`. Dense regions classify
conservatively per region — a reference/COW deopt reason reports
`unknown_aliasing`; every other supported region reports `no_references_observed`
— and then refine per slot: a register whose latest in-block definition is a
provably scalar-producing opcode is downgraded to the finest
`no_references_observed` class even inside a reference/COW region (see
Live-State Precision). Exact runtime distinctions (local-only vs escaped vs
global vs property/array-dim) still need runtime values.
`LiveStateSnapshot::reference_alias_summary` returns the coarsest class across
live slots, and the verifier rule `alias_metadata_consistent` rejects a snapshot
that reports no reference/COW control state yet carries a reference-sensitive
slot, keeping the summary honest before a future tier consumes it.

Future Cranelift, baseline-native, quickening, or trace-JIT work must consume
this VM-owned metadata or extend it with equivalent tests before it can claim a
safe mid-region resume.
