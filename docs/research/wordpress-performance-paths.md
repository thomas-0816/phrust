# WordPress Performance: Paths to Beating Native PHP

Date: 2026-07-07.

Reference target: PHP 8.5.7 (`php-8.5.7`).

This document records the measured WordPress performance gap, why it is where it
is, and the two multi-session paths that can close it. It is analysis and
planning evidence, grounded in measurements taken on the `phrustwordpress` Docker
stack and local release benchmarks. It does not itself change runtime behavior.

## The measured gap

The WordPress root request (warm, `phrustwordpress` Docker stack, clean
wall-clock) runs at **~1.19s on phrust vs ~100-150ms on native PHP — an ~8-12x
gap**. A per-request profile shows **100% of the time is VM execution**;
routing, session, and I/O are microseconds. The work is spread across roughly
1M VM operations per request (measured: 59,660 function calls, 8,211 method
calls, 1,488 object instantiations, 3,780,107 value clones, 10,957 COW
separations).

A before/after of a recent interpreter-optimization tranche (object-creation
cache, frame-shape memoization, dense property-dimension assignment, copy-patch
stencil prerequisites) measured **wall-clock neutral** on this request
(~1.20s -> ~1.19s). Those changes show 5-17% on 300K-repetition micro-benchmarks
but ~0% on WordPress, because WordPress is one-shot-distributed (low per-item
repetition) and the absolute per-operation savings (nanoseconds) vanish in a
request dominated by millions of already-cheap operations.

### Where the 10x actually is

Clean release measurements (`--jit off`, incremental cost over an arithmetic
baseline) locate the gap precisely:

| Operation | phrust | ~Zend | Verdict |
| --- | ---: | ---: | --- |
| arithmetic op | ~50 ns | ~40-60 ns | already competitive |
| array read/write | ~63 ns | ~50-80 ns | already competitive |
| function call | ~830 ns | ~50-100 ns | ~10x gap |
| object creation | ~1830 ns | ~150-300 ns | ~8x gap |

Dispatch and arithmetic are **not** the problem — they are already
Zend-class. **Function calls and object creation carry the ~10x.** Both paths
below are aimed there.

Native PHP serves WordPress bootstrap mostly through its *interpreter* plus
opcache (its JIT rarely fires on one-shot code), so an interpreter *can* reach
~100ms — Zend's does. That means the gap is closable in principle by interpreter
work, but it requires matching Zend's per-operation cost (~10x), which is a
ground-up program, or bypassing the interpreter with a JIT.

## Path A: Zend-class interpreter architecture (~10x)

### A1. Dispatch (smallest lever)

- phrust dispatches via a Rust `match instruction.opcode` (a bounds-checked jump
  table) with per-instruction overhead: `observe_dense_quickening` performs a
  `RefCell::borrow_mut` + hashmap probe on every instruction (pure waste for
  one-shot code), plus counter hooks.
- Zend uses computed-goto threading: each handler jumps directly to the next, so
  there is no loop-top re-dispatch and each opcode gets its own
  indirect-branch-predictor slot.
- Guaranteed tail-call threading is not yet stable Rust. Realistic phrust moves:
  make quickening observation cheap/skippable for one-shot units, and extend
  superinstructions to cut dispatch count. Lowest priority, since arithmetic is
  already ~50ns.

### A2. Value representation

- phrust `Value` is a 24-byte enum; arrays/strings/objects are `Rc<...>`. Every
  compound touch is a non-atomic refcount write; 3.78M clones/request are 3.78M
  refcount bumps plus a (gated but branchy) stats hook on the hot path.
- Zend ZVAL is 16 bytes with a refcounted body only for compound types; scalar
  copies are a plain memcpy.
- Moves: shrink `Value` 24->16 bytes (every register/local write pays the extra
  8), compile the clone-stats hook fully out in release, consider a separated
  refcount body. Constant-factor win, not a gap-closer alone (clones are cheap Rc
  bumps, ~20-40ms total).

### A3. Argument binding / call convention (the dominant call-path lever)

- phrust `prepare_arguments` allocates, per call, a `bound: Vec<Option<...>>`, a
  `prepared: Vec<PreparedArg>`, `frame_args`, `trace_args`, and `diagnostics`
  Vec, and iterates params (required count, variadic position), running
  named/variadic/default machinery even for trivial positional calls. With ~68K
  calls/request this is heavy malloc churn and is most of the ~830ns-vs-~80ns
  call gap.
- Zend pushes args directly onto the VM stack into the callee's pre-allocated CV
  slots — zero per-call heap allocation on the common path; named/variadic/type
  checks are inline fast-path branches.
- Moves (highest value, reused by the JIT): a fast path for simple positional
  untyped calls that binds `arg[i] -> param[i]` directly into frame locals,
  skipping the `bound` Vec and named/variadic/default logic; build `trace_args`
  lazily (only on backtrace capture); reuse a per-frame scratch buffer. These are
  correctness-critical new paths (must replicate `prepared`/`frame_args`/
  `trace_args`/by-ref/type-check exactly) and require focused arg-binding PHPT
  validation before landing.

### A4. Object model

- phrust already caches the `runtime_class_entry` rebuild across instantiations
  (lineage walk, default eval, method mapping). Remaining per-`new` cost:
  per-property default clone into slots, layout lookup, and a string-keyed
  `__construct` lookup on every instantiation.
- Zend allocates the object and memcpy's a pre-computed default-slot vector from
  the cached class entry; the constructor is a cached function pointer.
- Moves: cache the constructor lookup per class (same pattern as the
  runtime-class cache); precompute a default-slot template per class and memcpy
  it; intern property names for O(1) slot lookup.

Path A is achievable in principle but is a multi-week program; A3 is the dominant
win, A4 second, A2 a constant factor, A1 the smallest, and some pieces
(computed-goto) fight stable Rust.

## Path B: Executable copy-and-patch JIT tier (the direct gap-closer)

Copy-and-patch compiles by copying pre-built machine-code stencils (one per
opcode/pattern) into an executable buffer and patching register indices, frame
offsets, branch targets, helper addresses, and immediates. Compile cost is
memcpy + patch, ~1us/function.

WordPress bootstrap is one-shot (no hot loops), so a threshold JIT like Cranelift
provably never fires (measured: the `experimental-jit` preset needs 8
entries/backedges; the JIT executes 0 times on WordPress; see
[copy-and-patch-stencil-tier.md](copy-and-patch-stencil-tier.md)). Cranelift's
~100us-1ms compile only pays off for hot code. Copy-and-patch's ~1us compile pays
off on the first call, so it can compile every function on first use and run
native thereafter — this is the mechanism that turns ~830ns interpreted calls
into near-native calls (Path A3 solved in hardware: args bind in registers/stack,
no per-call Vecs).

### Prerequisites

- Done: stencil classification for arithmetic, int comparison, bitwise/shift,
  property fetch/assign, known-builtin, branch, return; helper ABI hash;
  code-cache key schema; W^X policy; the verifier gate asserting
  `native_execution = false`.
- Remaining: typed live-state/deopt maps (exact register/local slots per patch
  point plus resume instruction), helper status contracts, frame-identity maps,
  reference/COW materialization rules, invalidation epochs, and the
  executable-memory emitter (mmap RW, write stencils, mprotect RX; never W^X
  simultaneously).
- Then: native execution behind guards, with deopt restoring exact interpreter
  state on guard failure.

Native execution stays gated (with an enforcing test) until every prerequisite
plus full PHPT/reference validation is closed. This gate is deliberate and must
not be rushed: writable+executable memory and imprecise deopt are
security/correctness hazards.

## Sequenced recommendation

Path B is the more direct gap-closer for WordPress specifically (its one-shot
nature is copy-and-patch's sweet spot), and its prerequisites are already being
built incrementally and safely (report-only). Path A is not wasted: the JIT
reuses the value model, call convention, and object model, so A3/A4 lower the
interpreter floor now and are reused by the JIT later.

1. A3 + A4 (arg-binding fast path, lazy `trace_args`, constructor cache,
   default-slot template) — real interpreter wins, reusable by the JIT,
   validatable with focused PHPT.
2. Path B prerequisites — deopt/live-state maps and helper contracts, staying
   no-exec.
3. The executable emitter — the actual gap-closer, behind the W^X and
   full-validation gate, as its own deliberately sequenced effort.

The interpreter tranche measured neutral on WordPress because WordPress is
one-shot; step 1's value is that it is reused by the JIT and helps hot-loop
workloads. Step 3 is what moves the WordPress number, and it is a multi-session,
security-gated effort — not reachable in a single session.

## See also

- [copy-and-patch-stencil-tier.md](copy-and-patch-stencil-tier.md)
- [php-mid-tier-compiler.md](php-mid-tier-compiler.md)
- [baseline-native-tier.md](baseline-native-tier.md)
- [deopt-resume-table.md](deopt-resume-table.md)
