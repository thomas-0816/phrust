# ADR 0019: Fast Baseline Native Tier Prerequisites

## Status

Accepted for fastest-engine prerequisite tracking. **Updated 2026-07-07:** the
executable-memory/W^X, helper-registry, ABI-hash, live-state, and
side-exit/deopt prerequisites below are now owned and tested, so the gate for a
**narrow guarded baseline-native subset** moves from `HARD_BLOCK` to
`SUBSET_ALLOWED` in `docs/performance/optimization-gates.md`. Broad generic
machine-code execution of whole functions stays `HARD_BLOCK` until the remaining
items (frame-model completion, source maps, foreach/exception/`finally`,
generator/fiber, diagnostics/output proof) and PHPT/reference parity gates are
satisfied. See "Prerequisite Status" below.

This ADR does not block interpreter-side subsets (dense dispatch, inline caches,
by-ref argument location encoding, optimizer passes); those follow the
`SUBSET_ALLOWED` and `EVIDENCE_GATE` policies in that document.

## Prerequisite Status (2026-07-07)

The mandatory-prerequisite table below is retained as the engineering checklist.
Current ownership:

| Area | Status | Owner |
| --- | --- | --- |
| Executable memory | Owned + tested | `crates/php_jit/src/code_memory.rs` (VM-owned `CodeMemory`, fail-closed `UnsupportedHost`, lifecycle/exec tests; the sole executable-mapping site). |
| W^X policy | Owned + tested | `code_memory.rs` per-platform transitions (macOS `MAP_JIT` + `pthread_jit_write_protect_np`; other-unix RW→RX `mprotect`), documented in `docs/performance/cranelift/safety-audit.md`. |
| ABI hash | Owned | `JIT_HELPER_REGISTRY_ABI_HASH` + `repr(C)` `JitCValue`/`JitCFrameView` in `crates/php_jit/src/abi.rs`. |
| Helper registry | Owned + tested | `crates/php_jit/src/helpers.rs` (`JIT_HELPER_SYMBOLS`: versioned ids, names, signatures, side effects, return-status). |
| Side exits and deopt records | Owned | `abi.rs` `SideExitReason` / `JitSideExit` / `JitCExit`. |
| Live-state map | Owned | `crates/php_jit/src/region_ir/osr.rs` `RegionOsrEntry.live_slots` over `VmSlotId`. |
| References and COW | Rejected (owned) | OSR motion policy rejects `reference_or_cow_state`; the guarded subset never executes reference/COW shapes. |
| Frame model | Partial | `JitCFrameView` + `VmSlotId` slot mapping exist; marshaling contract specified in `docs/research/copy-and-patch-stencil-tier.md` ("Frame-Local Slot ABI"). Full call-frame identity/return-slot wiring is open. |
| Code cache lifecycle | Open | No persistent native cache yet; keys schema defined in `copy-and-patch-stencil-tier.md`. |
| Source-map and traces | Open | Native program-point → IR/dense/source mapping not yet emitted. |
| Foreach / exceptions & `finally` / generators & fibers | Rejected (owned) | Rejected by the OSR motion policy and deopt metadata; not executed. |
| Diagnostics and output | Open | Warning/output-byte parity proof pending. |
| PHPT/reference gates | Open | Focused fixtures + PHPT/reference parity required before any default-on discussion. |

The guarded subset is permitted precisely because unproven shapes are
**rejected** (they take a typed side exit to the interpreter), never executed
with a plausible-but-wrong result.

## Context

The fastest-engine track needs a credible Sparkplug/YJIT-style baseline native
tier plan, but the repository must not grow a broad executable native engine
before the VM owns the state needed to leave and re-enter optimized code
without changing PHP behavior.

Current execution remains interpreter-first. Cranelift is feature-gated and
default-off. This ADR defines the prerequisites for any future executable
baseline-native tier and records the no-exec stencil evidence added for the
baseline-native research path.

## Mandatory Prerequisites Before Execution

Executable baseline-native code is blocked until all of these are owned,
documented, and covered by focused tests:

| Area | Required shape |
| --- | --- |
| Executable memory | One VM-owned code-memory abstraction with explicit platform support, fail-closed unsupported-host behavior, lifecycle tests, and no ad hoc `mmap` or `mprotect` call sites. |
| W^X policy | Documented write-then-execute transitions, no simultaneously writable/executable pages in owned code paths, and platform-specific tests for every supported host. |
| Code cache lifecycle | A process/request policy for allocation, finalization, invalidation, teardown, and stale-entry rejection. Persistent native caches require integrity, target, ABI, config, ISA, and epoch keys. |
| ABI hash | A stable hash covering value layout, frame layout, helper signatures, exit statuses, pointer width, target ISA, and baseline-native configuration. |
| Helper registry | Versioned helper ids, names, signatures, side effects, diagnostics behavior, allocation behavior, and return-status meanings. |
| Frame model | Interpreter-compatible call frame identity, register file ownership, local-slot mapping, return slots, and helper-visible frame views. |
| Source-map and traces | Native program points must map back to IR/dense bytecode, source spans, and trace/debug output without reordering diagnostics. |
| Side exits and deopt records | Every helper/status exit, guard failure, overflow, stale metadata case, exception marker, and bailout must have a typed reason and exact resume location. |
| Live-state map | Optimized points need representable registers, locals, temporaries, call-frame identity, current block/instruction, source span, and pending return value state. |
| References and COW | Reference cells, aliases, COW sharing, separation points, by-reference sends, by-reference foreach, and array/object identity must either be represented or rejected. |
| Foreach state | Iterator position, key/value slots, by-value/by-reference mode, mutation epoch, packed/mixed layout state, and resume semantics must be explicit. |
| Exceptions and `finally` | Native exits must preserve unwind order, pending exception state, `finally` execution, destructor order, and catch/finally resume targets. |
| Generators and fibers | Native entry/resume is rejected until suspended VM state and native live state can be represented without losing identity. |
| Diagnostics and output | Warning/error order, stdout/stderr bytes, output buffers, callbacks, object conversion, and binary string behavior must match interpreter execution. |
| PHPT/reference gates | Baseline-native behavior needs focused fixtures first, then runtime/PHPT gates and reference-PHP orientation before any default-on discussion. |

## No-Exec Stencil Prototype

The baseline-native report-only prototype is:

```bash
php-vm dump-baseline-native-stencil <file.php> --json
```

The command compiles PHP through the normal frontend, lowers verified IR to the
current dense bytecode subset, verifies dense bytecode invariants, and emits a
platform-neutral baseline-native stencil estimate. It does not allocate
executable memory, does not emit machine code, and does not add a runtime mode.

The JSON report includes:

- `status: "no-exec"`;
- `native_execution: false`;
- `executable_memory: false`;
- instruction, helper-call, deopt-slot, compile-cost, and code-size estimates;
- opcode counts;
- unsupported reasons for dense operations that need VM-owned live state before
  they can be stencilized.

Current unsupported reasons include userland call-frame side effects, array
reference/COW/key state, and foreach iterator state.

## Comparison To Existing Tiers

| Tier | Current role | Baseline-native implication |
| --- | --- | --- |
| Interpreter plus dense bytecode | Source of truth and correctness oracle for VM execution. | Future baseline-native work must consume dense bytecode; it must not add another parser, AST, semantic frontend, or string-matching execution path. |
| Quickening, inline caches, superinstructions | Safe near-term acceleration because execution remains inside the VM. | Continue expanding these while collecting fallback/deopt counters that future native tiers can reuse. |
| Cranelift selective regions | Default-off native subset for proven hot regions with helper ABI, side exits, blacklist, and guard reports. | Keep selective and feature-gated. Do not use Cranelift as an excuse to skip baseline-native executable-memory and live-state prerequisites. |
| Baseline-native stencil | No-exec evidence for dense bytecode suitability, compile cost, code-size estimates, helper pressure, and unsupported state. | Useful as planning data only. It cannot justify native execution until every prerequisite above is satisfied. |
| Copy-and-patch stencil library | No-exec textual stencil records over quickening-compatible dense bytecode, including patch sites, guards, helpers, live-state needs, side exits, unsupported reasons, code-size estimates, and work-to-compile ratio. | Useful for deciding whether a future low-latency baseline tier is worth its prerequisites. It still cannot allocate executable memory or bypass live-state/deopt requirements. |
| PHP-aware mid-tier plan | Metadata-only design and report over dense bytecode, IC feedback, shape metadata, numeric-string feedback, branch bias, persistent feedback, and deopt/live-state requirements. | Useful for deciding when guard sharing, shape hoisting, and PHP-specific specialization justify a tier above stencils and below Cranelift. It cannot execute until the same live-state, deopt, invalidation, and PHPT prerequisites are satisfied. |
| Region profile report | Metadata-only framework trace-shape evidence from VM counters, IC states, source maps, and shape summaries. | Useful for ranking future inline-cache, superinstruction, baseline-native, and Cranelift candidates. It does not satisfy executable-memory, live-state, deopt, exception, generator/fiber, or PHPT prerequisites. |

## Validation

The focused no-exec gate is:

```bash
nix develop -c just baseline-native-stencil-smoke
nix develop -c just copy-patch-stencil-smoke
nix develop -c just mid-tier-plan-smoke
```

The broader native-tier prerequisite gates are:

```bash
nix develop -c cargo test -p php_jit -p php_vm
nix develop -c just safety-audit-smoke
nix develop -c just verify-performance
```

## Decision

A **narrow guarded baseline-native subset** may execute now that the
executable-memory/W^X, helper-registry, ABI-hash, live-state, and
side-exit/deopt prerequisites are owned and tested (see "Prerequisite Status").
Such execution must: consume verified dense bytecode only; marshal live slots
in/out through the VM; guard every specialized shape; take a typed side exit to
the interpreter on any unproven shape (references/COW, foreach, exceptions,
`finally`, generators, fibers, dynamic calls); and stay **default-off** behind
the JIT feature gate with fallback/side-exit counters intact.

Broad generic machine-code execution of whole functions stays blocked until the
remaining prerequisites (frame-model completion, code-cache lifecycle, source
maps, foreach/exception/`finally`/destructor materialization, generator/fiber
snapshots, diagnostics/output-byte proof) and PHPT/reference parity gates are
implemented and accepted. No default-on discussion happens before that.
