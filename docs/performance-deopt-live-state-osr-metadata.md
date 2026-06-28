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
  output-buffer marker, and call-frame identity marker.
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
- operand-stack slots are rejected because the current VM is register-based.

Dense bytecode structural validation remains owned by `DenseBytecodeUnit::verify`.

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
- compatibility of VM deopt reason codes with the existing Cranelift side-exit
  prefix.

## Remaining Boundaries

This foundation is not a complete OSR implementation. The following remain
blocked until future focused work:

- native frame reconstruction from arbitrary instruction points;
- precise initialized/uninitialized live-value liveness;
- exact reference alias classes and array element identity maps;
- pending exception object materialization;
- finally/destructor ordering across optimized exits;
- generator/fiber suspension snapshots;
- output-buffer callback state;
- cross-region trace linking and invalidation.

FPE-28 region profiles consume current VM counters and IR/source-map metadata
to classify future candidate regions, but they do not add the missing resume
state above. Their exception, `try`/`finally`, generator, and fiber entries are
rejection metadata, not executable deopt support.

Future Cranelift, baseline-native, quickening, or trace-JIT work must consume
this VM-owned metadata or extend it with equivalent tests before it can claim a
safe mid-region resume.
