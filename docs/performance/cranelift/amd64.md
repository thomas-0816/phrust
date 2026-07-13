# AMD64 Linux Cranelift Contract

The production experiment target is `x86_64-unknown-linux-gnu` using the
System V AMD64 ABI. Cranelift is the native backend on this target. The
copy-and-patch tier remains an aarch64/Unix implementation and must not be
credited in AMD64 reports.

The runtime default remains managed execution with JIT disabled. AMD64 native
execution is opt-in through the `jit-cranelift` Cargo feature and the
`experimental-jit` engine preset.

## Lean Build

Build the clean candidate without default features:

```bash
nix develop -c just release-cranelift-amd64
```

This writes `target/amd64-cranelift/release/phrust-server` by default. The
equivalent direct build is:

```bash
CARGO_TARGET_DIR=target/amd64-cranelift \
  cargo build --release -p php_server --bin phrust-server \
  --no-default-features --features jit-cranelift
```

Run the Cranelift build directly:

```bash
target/amd64-cranelift/release/phrust-server \
  --docroot "$PHRUST_WORDPRESS_DOCROOT" \
  --front-controller index.php \
  --deployment-mode immutable \
  --engine-preset experimental-jit
```

## Native Smoke

```bash
nix develop -c just jit-smoke-amd64
```

The gate executes every currently supported Cranelift candidate through the VM
tests and writes `target/performance/jit-smoke.json`. It fails unless all of the
following are true for the executable fixture:

- `jit_mode == "cranelift"`;
- `jit_compiled > 0`;
- `jit_executed > 0`;
- `native_platform_unavailable == 0`;

The report also contains the host-native Cranelift ISA display, ISA-feature
fingerprint, runtime ABI hash, JIT configuration hash, code size, and compile
time. Host ISA features are part of the compile-cache target identity.

## WordPress A/B

```bash
nix develop -c just wordpress-root-benchmark-cranelift-amd64
```

The recipe builds three source-identical binaries:

- `target/amd64-baseline`: lean managed baseline, JIT off;
- `target/amd64-cranelift`: lean Cranelift candidate;
- `target/amd64-cranelift-diagnostic`: telemetry-enabled Cranelift binary used
  only for untimed native evidence.

The A/B has exactly two clean AMD64 timing arms:

| Arm | Engine preset |
| --- | --- |
| `managed-baseline` | `default` |
| `cranelift` | `experimental-jit` |

Clean timing and instrumented diagnostics are separate. Reports include the
source commit, uncommitted patch SHA-256, binary hashes, `platform.machine`,
Rust target triple, Cranelift version, CPU vendor/model/family/stepping, host
feature fingerprint, target ISA display, JIT configuration hash, and runtime
ABI version/hash.

WordPress and the PHP-FPM 8.5.7 reference must be configured as described in
[benchmark methodology](benchmark-methodology.md). Missing external benchmark
prerequisites produce a visible skip in non-strict mode; strict tranche runs
fail instead of silently omitting the measurement.

For an already recorded managed baseline, run the class-B acceptance gate:

```bash
nix develop -c just \
  wordpress-root-tranche-gate-cranelift-amd64 \
  target/performance/wordpress-root/baseline.json
```

This requires the standard warm concurrency-1 p50 improvement and keeps the
existing correctness, PHP-control-drift, and clean-timing contracts.

## Scalar regions and calls

The AMD64 backend's scalar path builds backend-neutral executable Region IR
from authoritative `php_ir` and lowers that verified multi-block graph, rather
than selecting a named arithmetic leaf. It accepts exact integer parameters/returns, locals,
register moves, constants, checked arithmetic, comparisons, conditional and
unconditional branches, loops, and returns. A region with no arithmetic at all
(for example an identity wrapper) still compiles and executes, proving that a
verified supported CFG is not rejected merely because it missed a leaf
recognizer. PHP shapes outside that exact contract continue in Dense.

Stable same-unit direct calls are collected as one bounded native call graph.
All functions are declared before definition (including recursive edges), and
Cranelift emits direct compiled-to-compiled calls. The owning process handle
keeps the complete graph alive. Native exits identify both the callee function
and exact continuation and materialize definitely-live scalar locals. Native
PC ranges and loop OSR entries are published with the handle. The counter
`compiled_to_compiled_calls` records successful native call linkage.

Eligible Dense function, method, static-method, and constructor calls transfer
caller operand sources directly and resume through the iterative activation
trampoline. These calls do not construct nested `VmResult` values and deep PHP
call chains do not grow the Rust stack.

## Tiering and bounded prewarm

Dynamic exits use a minimum execution sample and exit/guard-failure rates.
Layout-independent handles survive class-table epoch changes; layout-sensitive
property handles remain epoch-bound. A poor specialization enters a bounded
128-call cooldown and can re-enter after the cooldown or an epoch change.
Compiler errors and ABI mismatches remain strict.

`--script-cache-preload` also performs bounded Cranelift prewarming when the
experimental JIT preset is active. Each artifact is limited to 64 functions or
10 ms, application code is not executed, and published handles live in the
process code manager so later workers adopt the same code generation. Startup
prewarm is therefore outside timed HTTP samples; runs without an explicit
preload manifest still rely on normal warmup and must report compile work.

The metrics endpoint exposes the readiness/quiescence contract used before a
timed run:

- `phrust_server_script_cache_ready`
- `phrust_server_jit_prewarm_complete`
- `phrust_server_jit_compile_queue_empty`
- `phrust_server_jit_code_cache_generation`
- `phrust_server_jit_prewarm_entries_total`
- `phrust_server_jit_prewarm_nanos_total`

The first three gauges must be `1`. Prewarm time is startup time and must not be
folded into steady-state HTTP samples.
