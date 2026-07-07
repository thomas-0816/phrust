# Native-Tier + Runtime Roadmap: Toward Native-PHP Parity

Date: 2026-07-07.

Reference target: PHP 8.5.7 (`php-8.5.7`).

This is the combined roadmap for two efforts that must run **in parallel**: the
copy-and-patch **native tier** (making phrust execution as native as possible)
and the **runtime tranche** it depends on. Neither alone closes phrust's gap to
native PHP on real web workloads — the native tier removes dispatch overhead, the
runtime tranche makes the work each operation does cheap, and the two multiply.
This document is planning/research evidence; it does not change runtime defaults
and does not itself add executable code.

## What "as native as possible" means

You do not compile a dynamic language like PHP to *pure* machine code. Every
production PHP/Ruby JIT (Zend JIT, YJIT, V8 Sparkplug) is a **baseline JIT**:
native code that

1. runs the common **typed fast paths inline** — integer/float arithmetic,
   guarded property/array/method access;
2. **calls VM helpers** for everything heap-y or side-effectful — allocation,
   string/array operations, most builtins, slow paths;
3. **guards its assumptions and deopts** to the interpreter on surprise.

The speed comes from eliminating per-operation *dispatch* (decode, opcode
dispatch, operand shuffling) and inlining hot paths — not from avoiding the
runtime. This is the central distinction that governs every estimate below:

> **Dispatch vs. runtime.** The native tier eliminates dispatch. It does *not*
> eliminate runtime work (allocation, COW cloning, array hashing, string ops,
> object setup), which it performs through the same VM helpers. A workload's
> speedup is proportional to how much of it is dispatch/fast-path versus runtime.

Realistic ceiling for hot code: ~5–15× over a good interpreter. The ~100× we
measured on a pure-integer loop (below) is the extreme upper bound — a loop with
zero runtime interaction. Real mixed code lands far lower, and real *web* apps
lower still (see "Expected speed results").

## Current state

Landed this tranche (all default-off, behind the `jit-copy-patch` feature +
`PHRUST_JIT_COPY_PATCH` env, aarch64 only): a working guarded native tier for
scalar-int leaf functions and canonical counted `for` loops, recognized from
real IR and executed end-to-end through the VM. The two sections that follow are
grounded against the actual code; the measured results are the anchors:

- native scalar-int op ≈ **2.05 ns/op** vs the interpreter's ~50 ns/op;
- native counted loop ≈ **1.09 ns/iter** vs the interpreter's ~200 ns/iter;
- `sum_to(int $n): int` (a real PHP `for` loop) recognized and run **natively**,
  `sum_to(30_000_000)` correct in ~289 ms total (native loop ~32 ms);
- a per-call scalar-int **leaf** ≈ 1.7× — small, because a leaf only removes its
  own body's dispatch while the caller's loop stays interpreted. Loops are the
  lever; leaves are not.

## Native tier: state and roadmap

The native tier is a copy-and-patch (template-JIT) experiment that lowers a
narrow, proven-shape subset of PHP functions to real aarch64 machine code
executed over a flat `JitCValue` slot buffer. It is **default-off**: gated behind
the `jit-copy-patch` cargo feature *and* the `PHRUST_JIT_COPY_PATCH` env var
(`crates/php_vm/src/copy_patch_bridge.rs:116`), and it only emits code on
`unix + aarch64`. The design intent (W^X memory, slot ABI, "compile once / run
many") is spelled out in `docs/adr/0019-fast-baseline-native-tier-prerequisites.md`
and `docs/research/copy-and-patch-stencil-tier.md`.

### What's landed

**Executable memory (`crates/php_jit/src/code_memory.rs`).** `CodeMemory` is the
sole VM-owned path for emitted machine code and upholds W^X: on Apple Silicon it
maps `MAP_JIT` pages and toggles `pthread_jit_write_protect_np` (write, flip to
execute, invalidate i-cache); other Unix hosts map RW, copy, then `mprotect` to
RX; unsupported hosts fail closed (`CodeMemoryError::UnsupportedHost`). Nothing in
the interpreter calls it except through the copy-patch tier.

**aarch64 emitter (`crates/php_jit/src/aarch64.rs`).** A hand-rolled little-endian
encoder (`Aarch64Assembler`) with forward/backward branch fixup resolution in
`finish`. Instruction families: immediates/moves (`movz`, `movk`, `mov_imm64`,
`mov`); arithmetic (`add`, `adds`, `sub`, `subs`, `mul`, `smulh`); guards/branches
(`cmp_imm_w` tag guard, `cmp_shifted_asr63` mul-overflow check, `cmp_reg`, signed
`Cond` Overflow/Eq/Ne/LT/LE/GT/GE via `b_cond`, unconditional/backward `b` for loop
back-edges); loads/stores over the slot buffer (`ldr_w`/`ldr_x`, `str_w`/`str_x`);
and calls/frame ops (`blr`, `push_fp_lr`/`pop_fp_lr`, `ret`). **Note:** the
call/frame ops exist in the encoder but are not yet emitted by any stencil (gap b).

**Stencils + recognizers (`crates/php_jit/src/copy_patch.rs`).** Slot ABI:
`JitCValue` is 24 bytes, slot `i` at `i*24`, tag guard bounds addressing to
`MAX_SLOT = 4095/6`. Per-op emission is `emit_op` (guarded via `emit_int_guard`
and `emit_store_int`), covering `ScalarIntOp` = `Const` and guarded `Binary`
`Add`/`Sub`/`Mul`; each binary op emits `Int`-tag guards on both operands and an
overflow side exit. Straight-line sequences: `emit_scalar_int_ops`. Loops:
`CountedLoop` + `emit_counted_loop` emit a full native
`while slot[counter] < slot[limit] { body; counter += 1 }` with the whole loop —
condition, body, increment, back-edge — running natively (loop-carried values live
in slots, so no phi handling or register allocation). IR-level recognizers:
`build_scalar_int_region` (single-block `: int` free function of `int` by-value
params: `LoadLocal`/`LoadConst`/`Move`/`Binary Add|Sub|Mul` + `Return`) and
`compile_counted_loop_function` (the canonical 5-block for-loop CFG: prologue →
header `i < n` → body accumulator → `i++` increment → exit return).
`compile_scalar_int_function` tries the leaf shape first, then the counted loop.

**ABI (`crates/php_jit/src/abi.rs`).** `JitCValue` (repr(C), tagged),
`SideExitReason` (stable report codes / resume metadata), `JitCExit` (returned /
bailout / side_exit / with_resume), versioned by `JIT_RUNTIME_ABI_VERSION`/`_HASH`.

**Helper registry (`crates/php_jit/src/helpers.rs`).** A *stable helper-symbol ABI*
(`JitHelperSymbol`, ids, arg/return kinds, status codes) exists as scaffolding for
a native↔helper boundary — but no stencil calls into it yet.

**Region IR + OSR metadata (`crates/php_jit/src/region_ir/`).** `builder.rs` builds
the sea-of-nodes graph the compiler lowers. `osr.rs` computes *metadata only*
(`RegionOsrEntry.live_slots`, `select_region_osr_entries`,
`region_osr_motion_policy`) — unwired to any real resume (gap c). `eligibility.rs`
is a separate conservative analysis that "deliberately accepts only a tiny
primitive, leaf-function IR subset."

**VM integration.** `crates/php_vm/src/copy_patch_bridge.rs` marshals frame values
into the slot buffer and back (`marshal_local`, `unmarshal_result`), owns the
compile-once/run-many `NativeLeaf` and a thread-local `cached_leaf` cache keyed by
`(unit_id, function_id)`, and reads the env gate `copy_patch_leaf_enabled`. The
hook `try_execute_copy_patch_leaf` (`crates/php_vm/src/vm/mod.rs`) is invoked from
the **rich** executor `execute_function_inner`, deliberately *before* dense
dispatch so a recognized leaf runs natively.

Relevant commits: `6f4be9de` (code memory), `daf32e1c` (first end-to-end emitter),
`14b12369`/`7a94adaf` (guarded int-add over the slot ABI), `62f8e4b3`/`71776da3`
(subs/smulh; widen to sub/mul/const), `55f3034e` (leaf recognizer),
`38d8ead7`/`8a8273e0`/`70c88c82` (VM bridge + hook + arg-gate fix), `51bc38f6`
(native counted loops), `c85846fc` (recognize real PHP for-loops), `5b3627f8`
(int `Compare` → bool), `ad787549`/`2b539865` (bitwise, const operands, `Mod`,
shifts), `40fe9238` (general CFG compiler — gap a), `79420c51` (dense-path hook —
gap d), and float arithmetic (gap e, float portion).

### Gaps to "as native as possible"

**(a) General control-flow compiler.** — **LANDED** (`compile_scalar_int_cfg`).
A general CFG compiler now lowers arbitrary `if`/`else`, `while`, early return, and
non-unit-increment loops: every SSA register and local gets its own slot, so control
flow is just branches between per-block labels with no phi handling or cross-block
register allocation. The leaf and counted-loop recognizers stay ahead of it (tighter
code); it is the catch-all fallback for the int/bool subset. `foreach`, nested-loop
edge cases, and the sea-of-nodes `opt/` passes on the emit path remain future work.

**(b) Calls + a real helper ABI — the keystone.** Native code cannot call another
PHP function or a builtin. The encoder has `blr`/`push_fp_lr`/`pop_fp_lr` and
`helpers.rs` defines a stable helper-symbol ABI, but no stencil emits a call and no
helper is registered/linked. Every recognizer rejects `Call`. This is the
prerequisite for anything resembling real WordPress code.

**(c) Mid-region deopt / OSR.** Deopt is **entry-only**: a guard/overflow side exit
returns `1` and `NativeLeaf::run` returns `None`, so the whole call falls back to
the interpreter from the top. `region_ir/osr.rs` computes live-slot metadata but it
is **unwired** — there is no real resume-at-program-point.

**(d) Default-mode engagement.** — **LANDED** (dense-path hook). The dense
executor's fast CALL dispatch site (`DenseFunctionPlan::Dense`) now tries the
native leaf before falling through to `execute_bytecode_function`, so the tier
engages on dense (default-mode) code, not just `--exec-format ir`. Still gated
behind the `jit-copy-patch` feature + `PHRUST_JIT_COPY_PATCH` env, verified by a
differential harness (`just copy-patch-native-diff`) that diffs native-on vs
native-off vs PHP 8.5.7. Flipping the env default to on awaits broader coverage
(calls/objects) and the mid-region deopt safety net (gap c).

**(e) Type/operator/data coverage.** — **PARTIALLY LANDED.** Integer operators are
now complete: `Add`/`Sub`/`Mul` (overflow-guarded), `Mod` and shifts (domain-guarded),
bitwise `And`/`Or`/`Xor`, integer `Compare` → bool, and constant right-operands.
**Float arithmetic** is landed too: double-precision `fadd`/`fsub`/`fmul`/`fdiv`
over FP registers, guarded by a `FloatBits` tag check with a zero-divisor side exit
for `/` (float `/` is float-typed, so it *is* in the subset, unlike int `/` which
may produce a float). Still missing: float `Compare` and float loops/branches;
`Concat` and boolean logic; strings/arrays/objects — all of which route through the
helper ABI (gap b) and so are gated on it. Any uncovered shape forces interpretation.

**(f) x86-64 backend.** The emitter is aarch64-only; `copy_patch_bridge.rs` and the
VM hook are `#[cfg(all(unix, target_arch = "aarch64"))]` and fall back everywhere
else. No x86-64 or Windows codegen path exists.

## Runtime tranche: state and levers

The native tier removes dispatch; this tranche makes the work each operation does
cheap. It is the co-requisite for beating native PHP on WordPress, because WP's
cost is *runtime*, not dispatch.

### Where WordPress time actually goes

Clean per-operation measurement (release, `--jit off`, caches off), stated as
measured facts:

| Operation | phrust | native PHP |
| --- | --- | ---: |
| integer arithmetic | ~50 ns | — (fine) |
| packed-array write `$a[$i]=$i` | ~63 ns | — (fine) |
| function call | ~830 ns | ~50–150 ns |
| method call | ~940 ns | ~50–150 ns |
| object creation `new P()` | ~1830 ns | ~50–150 ns |

So arithmetic and packed arrays are already competitive; **calls (~10–16×) and
object creation (~35×) are the gap.** WordPress is a call- and object-heavy OOP
framework, so its ~1.2 s/request is dominated by these.

**Correction to record (important):** value "clones" are *not* the bottleneck.
`PhpArray`/`PhpString`/`ObjectRef` are `Rc`, so a clone is a ~1 ns refcount bump —
the 3.78M clones/request measured with instrumentation ≈ ~20–40 ms, and the
"clones dominate" reading was an artifact of the source-attribution instrumentation
itself (thread-local borrows per clone). Clone *count* is an overrated proxy; the
**absolute per-op cost of calls and object creation** is the real lever. What
matters in the clone data is the *content* copies (COW separation deep-copying
array/string contents), not the refcount bumps.

### What's landed (and a critical lesson)

- **A3 — arg-binding fast path** (`crates/php_vm/src/vm/arguments.rs`,
  `bind_arguments`): plain positional, exact-arity, untyped-by-value calls skip the
  per-call `bound: Vec<Option<CallArgument>>` allocation and the named/variadic/
  default machinery. ~4.3% on 500K 2-arg calls.
- **Runtime-class-entry cache** (`vm/mod.rs` `runtime_class_entry`, guarded by
  `ExecutionState::class_table_epoch`): the per-`new` rebuild of `RuntimeClassEntry`
  (lineage walk + default eval + method/constant maps) is Rc-cached per class.
  ~16.8% on 300K instantiations.
- **Frame-shape memoization** (`FrameShapeFlags`): per-call body-scan
  classification (try/finally, destructor-sensitivity, inline blockers) is memoized
  per `(unit, function)` instead of re-scanned every call. ~5.6% on 100K calls.

**Critical lesson baked into every runtime item:** per-request memoization does
*not* help WordPress. WP is one-shot-distributed — functions run 1–2× per request,
so a per-`Vm` cache is cold each request and adds lookup overhead for no reuse. The
object-cache (17% on micro-benchmarks) and frame-shape work (5.6%) showed **~0% on
the actual WordPress request**. The levers below must cut the **absolute per-op
cost**, not add caches.

### The levers (R1–R5, WordPress priority order)

| | Lever | What it is | Rough per-op goal |
| --- | --- | --- | --- |
| **R1** | **Call-frame cost** *(the #1 WordPress lever)* | Frame pooling/reuse, extend the A3 lean arg-binding, build `trace_args` **lazily** (only on backtrace capture, not per call), strip per-call counter bookkeeping from the hot path, minimize `RegisterFile`/`LocalFile` setup in `frame.rs` (`FrameActivationContext`, `push_fresh_frame`/`push_reusable_frame`) | 830 → ~150–250 ns |
| **R2** | **Object-creation cost** | **Rc-wrap `php_ir::ClassEntry`** so lookups return `Rc<ClassEntry>` and the multiple deep-clones per `new` (the explicit `class.clone()` plus `ResolvedMethodOwned` in `lookup_resolved_method_in_state`) become refcount bumps — the diagnosed real object lever for WP's big classes; lean property-default init; keep the runtime-class-entry cache | 1830 → ~300–500 ns |
| **R3** | **Value moves / COW contents** | Last-use *move* instead of clone where a liveness pass proves single-use (widen `take_consumed_dense_operand`, used rarely today); avoid deep-copying array/string *contents* on COW separation when the source is dead. Needs a real last-use/liveness pass — the IR verifier confirms def-before-use but **not** single-use, so register reads cannot be blindly converted to moves | cut the content-copies + register-move copies |
| **R4** | **Allocation model** | Request-local arenas + frame/register pools (`docs/research/request-local-arenas.md`, `runtime-layout-compactness.md`) to cut malloc pressure across ~1M ops/request; reset per request with strict teardown/destructor equivalence | fewer allocs/request |
| **R5** | **Array/property/string fast paths** | Packed arrays already good (~63 ns); shape-IC property fetch/assign (partly landed); interned string-key hashing; keep these competitive as coverage grows | steady |

R1 and R2 dominate the WordPress budget and are the ones that make native calls
worth building — a native call is only fast if the frame/arg/alloc work beneath it
is also fast.

## Expected speed results (vs. phrust's current interpreter)

| Workload | Expected speedup | Why |
| --- | --- | --- |
| Compute-heavy (numeric loops, parsers, algorithms, encoding) | **5–15×** | Dispatch dominates; native eliminates it. The pure-int loop (~100×) is the extreme; real mixed compute is lower. |
| Typical web app (WordPress, Laravel, Symfony) | **~1.3–2×** | Time is dominated by calls, object creation, array/string ops, and allocation — mostly *runtime*, which stays helper-bound. Native cuts dispatch and inlines thin fast paths; the bulk does not accelerate. |

**Industry reference points (real applications, not synthetic):** Zend JIT
delivers 2–3×+ on compute benchmarks but famously ~0–5% on real
WordPress/Laravel — web apps are call/object/I/O-heavy, not compute-bound. YJIT
gets ~15–40% on real Rails apps and that is a major success. Single-digit-to-~50%
over an already-fast interpreter is the norm for average web PHP.

## Combined trajectory (rough WordPress request time)

Native PHP runs the reference WordPress request in ~100–150 ms; phrust today is
~1.2 s (~8–12× slower). The tranches multiply:

| Stage | Est. WP request | vs. native PHP (~100–150 ms) |
| --- | --- | --- |
| Today | ~1.2 s | ~8–12× slower |
| + Runtime tranche (R1+R2 dominate: calls ~200 ns, objects ~400 ns) | ~400–600 ms | ~3–5× slower |
| + Native tier covering calls/objects/arrays (dispatch elimination atop the lean runtime) | ~250–400 ms | ~2–3× slower |
| Both fully mature + aggressive (arenas, specialization, native fast paths on the hot path) | ~120–200 ms | ~parity; beating it is a stretch |

These are order-of-magnitude estimates from the measured per-op costs, not
promises. The point is the *shape*: a native call is only fast if the underlying
frame/arg/alloc work is also fast, so R1 (call-frame cost) and native-calls are
complementary, not redundant.

## Honest assessment: can phrust beat native PHP on WordPress?

- **Getting from ~10× slower to ~2–3× slower is very achievable** with both
  tranches — a solid, defensible result.
- **Beating native PHP (opcache, no JIT) on WordPress is possible but at the
  aggressive edge, and not guaranteed.** Native PHP's threaded C interpreter and
  `zval` runtime are genuinely lean; phrust's `Rc`+COW value model carries
  overhead a Rust engine must out-engineer with arenas and specialization. Parity
  is a realistic target; *beating* it needs both tranches fully mature and
  phrust's runtime leaner than PHP's C runtime on the hot path — reachable, but
  the last ~2× is the hardest.
- **The one place phrust likely beats native PHP outright is compute-heavy code**
  (the 5–15× native-tier zone), where native PHP's JIT is off by default.

## WordPress-specific ordering

WordPress has almost no pure-int hot loops, so our loop win does not reach it.
To move the *WordPress* number specifically the priority collapses to:

1. **Calls** — native→native, native→builtin (via helpers), native→userland;
   and the runtime **R1** call-frame cost. WordPress is call-heavy.
2. **Objects/properties** — shape-guarded slot access + IC method dispatch (via
   helpers); and runtime **R2** (Rc-wrap `ClassEntry`, lean property-default
   init). WordPress is OOP.
3. **Arrays** — packed-array fast paths inline, the rest via helpers. WordPress
   is array-heavy.
4. **Eager compilation.** WordPress is one-shot (functions run 1–2× per request,
   no hot loops), so a *threshold* JIT never fires. This is exactly why
   copy-and-patch is the right backend: ~µs/function, compile on first call.

Scalar loops and floats matter for compute-heavy PHP, not for WordPress.

## Sequencing recommendation

Run the two tranches **in parallel**; they gate different things.

1. **Runtime R1 (call-frame cost)** — the biggest single WordPress lever, and it
   is what makes native calls worth building.
2. **Native-tier calls + a real helper ABI, plus the general control-flow
   compiler** — the native keystone that turns "loops in isolation" into "real
   functions run native," and the gateway to object/array coverage.
3. **Runtime R2 (object creation / `ClassEntry` Rc) + native object/property
   coverage** — WordPress's OOP cost, attacked from both sides.
4. **Default-mode (dense-path) engagement + mid-region deopt/OSR** — so the tier
   fires by default and stays correct as coverage (and guard-failure frequency)
   grows.

## Measurement discipline

- Measure against the **WordPress Docker request** (root/warm, container rebuilt
  from the `phrust2` checkout), *not* micro-benchmarks. Per-request memoization
  and micro-benchmarks lie for one-shot WordPress: the object-cache and
  frame-shape work showed 17% / 5.6% on micro-benchmarks and ~0% on WordPress.
- Measure under **low host load**; this host is prone to load spikes that make
  wall-clock unreliable.
- **Timing-wrapper gotcha:** `/usr/bin/time`, `time (subshell …)`, and grep-pipe
  timing wrappers stall under load on this host. Run the binary **directly**
  (output to a terminal or file) or time with inline `date +%s%N` (no subshell).
  The native execution itself is fine; only the wrappers hang.
- Wall-clock alone never satisfies a performance claim: pair it with counters,
  fallback/side-exit attribution, and parity against PHP 8.5.7 output.

## See also

- [copy-and-patch-stencil-tier.md](copy-and-patch-stencil-tier.md) — the tier's
  stencil format, Frame-Local Slot ABI, and prerequisites.
- [wordpress-performance-paths.md](wordpress-performance-paths.md) — the two
  acceleration paths and where WordPress time goes.
- [../performance/deopt-live-state-osr-metadata.md](../performance/deopt-live-state-osr-metadata.md)
  — the deopt/live-state metadata the mid-region OSR step consumes.
- [request-local-arenas.md](request-local-arenas.md) and
  [runtime-layout-compactness.md](runtime-layout-compactness.md) — runtime
  allocation and value-layout levers (R3/R4).
- [../adr/0019-fast-baseline-native-tier-prerequisites.md](../adr/0019-fast-baseline-native-tier-prerequisites.md)
  — the gate and prerequisite status for the native tier.
