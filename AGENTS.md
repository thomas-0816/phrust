# Project Guidelines

## Repository Workflow

- Inspect the repository before changing files.
- Use `nix develop -c ...` for validation commands.
- Complete every change with relevant checks and report skipped checks clearly.
- If a check cannot run because of missing network, missing reference binaries,
  or platform support, report the skipped check and exact reason.
- Do not silently skip checks.
- Keep scripts deterministic and provide clear error messages.
- Use `bash` scripts with `set -euo pipefail`.
- Make script files executable when they are added.
- Update documentation together with tooling changes.
- In a dirty worktree, stage only files intentionally changed for the current
  task and never revert unrelated user changes.

## Reference Target

- PHP series: `8.5`
- PHP version: `8.5.7`
- Git tag: `php-8.5.7`
- Repository: `https://github.com/php/php-src.git`

Do not automatically update the target PHP version without a new ADR.

## Scope Boundaries

- Do not implement VM, runtime values, JIT, extensions, or Zend ABI emulation
  unless the user explicitly asks for that layer.
- Do not implement AST/HIR lowering, name resolution, compile-time semantics,
  constant expression evaluation, type checking, bytecode generation, or IR
  generation inside the parser/CST layer.
- Parser and CST work must reuse the existing lexer. Do not introduce a second
  lexer.
- Do not hardcode numeric PHP token values.
- Compare reference behavior by token names, token text, diagnostics, and
  source positions rather than raw numeric token IDs.
- Preserve byte-based spans as the source of truth. Treat line and column as
  derived display information.
- Public lexer and parser APIs must not panic on invalid input.
- Reference-dependent checks must skip clearly when no PHP reference binary is
  available and must be strict when `REFERENCE_PHP` is explicitly set.
- Do not commit generated reports under `target/`.
- Do not commit extracted `php-src` corpus files or a vendored `php-src` copy.
- Keep local reference checkouts under `third_party/`.

## Layer Boundaries

- Semantic work should consume `php_syntax` CST APIs and produce separate
  declaration tables, typed views, and semantic diagnostics.
- Parser diagnostics and semantic diagnostics should remain separate so parser
  acceptance stays comparable with the PHP lint oracle.
- Any execution layer must be introduced as a new bounded layer with its own
  validation gates and must not change lexer/parser contracts opportunistically.
- New tools should prefer existing source maps, token kinds, CST ranges, and
  fixture harnesses over adding parallel representations.

### Semantic Frontend

- Add typed AST views in a dedicated `php_ast` layer, not in `php_syntax`.
- Add HIR, declarations, scopes, name resolution, type lowering,
  constant-expression validation, attribute metadata, and semantic diagnostics
  in `php_semantics`.
- Keep `php_frontend_cli` as a consumer of `php_semantics`; do not add a second
  parser inside the CLI.
- Include, require, eval, function lookup fallback, attribute instantiation, and
  autoload-sensitive behavior must be represented as deferred metadata or known
  gaps, not executed.
- Every semantic diagnostic ID needs a fixture or an explicit reserved/known-gap
  note before it is considered complete.

### Runtime and VM

- Bytecode/IR, VM, and runtime work must consume HIR and semantic metadata
  through `php_semantics`; do not add a second lexer, parser, or semantic
  frontend.
- Keep `php_syntax` and `php_semantics` responsible for syntax and compile-time
  frontend diagnostics. Runtime diagnostics must live in the runtime/VM layer.
- Do not implement a full PHP standard library, Zend extension ABI, FPM/SAPI,
  Opcache, quickening, inline caches, or JIT as part of core runtime work.
- Unsupported runtime features must produce deterministic diagnostics or known
  gaps. Do not silently return plausible but incorrect results.

### Runtime Semantics

- Keep the only input pipeline as `php_lexer` -> `php_syntax` -> `php_ast` ->
  `php_semantics`/HIR -> `php_ir` -> `php_runtime` -> `php_vm` ->
  `php_vm_cli`.
- Do not add a second lexer, parser, AST, semantic frontend, or source
  string-matching execution path.
- Runtime semantics work focuses on references, Copy-on-Write, arrays, calls,
  objects, traits,
  enums, magic methods, generators, fibers, reflection, include/require/eval,
  autoload, globals, destructors, GC, diagnostics, and differential runtime
  fixtures.
- Every new runtime semantic behavior needs a focused fixture against
  `REFERENCE_PHP` when reference execution is available. Every unsupported
  language-semantics area needs a stable ID, fixture or concrete example, and
  known-gap documentation.
- Runtime semantics work does not imply a complete standard library, SPL, Zend
  extension ABI, FPM/SAPI, Opcache, quickening, inline caches, or JIT.

### Standard Library, Performance, and PHPT

- Standard-library work belongs in `php_std`, runtime builtin modules, or the
  owning VM integration point. Do not add generated registries or parallel
  builtin surfaces for temporary implementation history.
- Performance work may change caches, dispatch paths, optimizer passes, and
  measurement tooling, but it must preserve PHP-visible output, diagnostics,
  exit status, and side-effect order.
- PHPT tooling must treat source `php-src` tests as read-only inputs. Generated
  runnable PHPT fixtures belong under `tests/phpt/generated/`; run artifacts
  belong under `target/`.

## Validation Commands

- Use the narrowest relevant check while iterating.
- Use `nix develop -c just help` to discover the current canonical gates.
- Before finishing foundation, reference-tooling, lexer, parser, or CST work,
  run the strongest relevant verification target available in `just help`.
- Parser fixture, diff, and roundtrip gates should be run when available.
- For semantic frontend changes, prefer the narrow relevant gate first:
  `just semantic-fixtures`, `just semantic-diff`, or
  `just frontend-snapshots`.
- For runtime and VM changes, prefer `just bytecode-snapshots`,
  `just vm-smoke`, `just runtime-fixtures`, `just runtime-semantics-fixtures`,
  or `just runtime-semantics-diff` before broader gates.
- For standard-library changes, prefer `just stdlib-docs`,
  `just stdlib-coverage`, or the relevant `diff-*` gate before
  `just verify-stdlib`.
- For performance changes, prefer the focused smoke target that owns the
  optimization path before `just verify-performance`.
- For PHPT tooling or baseline changes, run `just verify-phpt`; use
  `just ci-phpt-smoke` for the CI runner-smoke contract.
- Before finishing broad cross-layer changes, run the matching aggregate gate:
  `just verify-frontend`, `just verify-runtime`, `just verify-stdlib`,
  `just verify-performance`, `just verify-phpt`, or `just ci-local`.
- Keep work vertical and auditable: requirement mapping, implementation,
  focused tests, then the relevant `nix develop -c just ...` gate.

## Performance Profiling

- Profile the dedicated `profiling` cargo profile, never the debug build:
  `nix develop -c cargo build --profile profiling -p php_vm_cli --bin php-vm`
  produces `target/profiling/php-vm` (release-equivalent codegen with
  line-table debug info for samplers).
- Host tools: `samply` (sampling CPU profiler, opens the Firefox Profiler UI),
  `oha` (HTTP load generation with latency histograms), `hyperfine` (available
  in the nix shell), and `xctrace`/Instruments on macOS for allocation traces.
  Install missing ones on macOS with `brew install samply oha`.
- Measure real applications in this order, so each step tells you where to
  point the next one:
  1. Phase and counter split with the built-in flags:
     `php-vm run --timings-json t.json --counters-json c.json <entry.php>`.
     Phases `frontend_analyze_ms`, `ir_lower_ms`, `execute_ms`, and
     `cache_load_ms` separate compile from execute; counters such as
     `includes`, `include_compile_misses`, and
     `rich_fallback_functions_executed` quantify how much application code
     runs uncached or outside dense dispatch.
  2. Single-request CPU profile:
     `samply record target/profiling/php-vm run <entry.php>`.
  3. Server under load: start the server, attach with
     `samply record -p <pid>`, then drive it with
     `oha -n 200 -c 4 <url>` for latency percentiles.
  4. Allocation breakdown when the sampler shows allocator dominance:
     `xctrace record --template 'Allocations' --launch -- <php-vm run ...>`.
- Instrumented runs (`--timings-json`/`--counters-json`) distort wall time;
  take timed comparisons from clean, uninstrumented runs and collect
  counters in a separate sample.

## Commit Message Rules

- Use conventional commits: `type(scope): description`.
- Keep the first line under 72 characters.
- Use imperative mood.
- Do not mention development provenance in commit messages.

## WASM cross-compilation (`wasm32-wasip2`)

```bash
CC_wasm32_wasip2=/tmp/wasi-sdk-33.0-x86_64-linux/bin/wasm32-wasi-clang \
AR_wasm32_wasip2=/tmp/wasi-sdk-33.0-x86_64-linux/bin/llvm-ar \
CFLAGS="-U SUPPORT_JIT" \
PCRE2_SYS_STATIC=1 \
cargo build --release --target wasm32-wasip2 -p php_vm_cli -p php_server
```

Run a `.php` file via Wasmtime (CLI):

```bash
wasmtime run --dir /tmp target/wasm32-wasip2/release/phrust-php.wasm /tmp/test.php
```

Run the built-in web server with `-S` (PHP-compatible CLI):

```bash
wasmtime run --dir /tmp -S tcp=y -S inherit-network=y \
  target/wasm32-wasip2/release/phrust-php.wasm -S 127.0.0.1:8080 -t /tmp
```

Or via the standalone server binary:

```bash
wasmtime run --dir /tmp -S tcp=y -S inherit-network=y \
  target/wasm32-wasip2/release/phrust-server.wasm --listen 127.0.0.1:8080 --docroot /tmp
```

### Browser demo (WASM via jco)

The browser demo at `examples/browser/` uses `@bytecodealliance/preview2-shim@0.19.0` (WASI polyfill)
and `@bytecodealliance/jco@1.25.0` (transpiler).

Build steps (wrapper script at `examples/browser/build.sh`):

```bash
# 1. Cross-compile phrust to wasm32-wasip2
CC_wasm32_wasip2=/tmp/wasi-sdk-33.0-x86_64-linux/bin/wasm32-wasi-clang \
AR_wasm32_wasip2=/tmp/wasi-sdk-33.0-x86_64-linux/bin/llvm-ar \
CFLAGS="-U SUPPORT_JIT" \
PCRE2_SYS_STATIC=1 \
cargo build --release --target wasm32-wasip2 -p php_vm_cli

# 2. Transpile to JS + core wasm with jco (no --map flags!)
examples/browser/build.sh
```

Run `build.sh` when the wasm component changes.

### Unavailable extensions on `wasm32-wasip2`

| Extension     | Reason |
|---------------|--------|
| `curl`        | Requires libcurl C library cross-compilation |
| `mysqli`      | `mysql` 28.x crate has wasm32 compilation issues |
| `openssl`     | Requires libssl C library cross-compilation |
| `pcntl`       | Requires POSIX signals (`SIG_*` constants) |
| `pdo_mysql`   | Same as `mysqli` |
| `pdo_pgsql`   | Same as `pgsql` |
| `pgsql`       | `tokio-postgres` 0.7.18 has wasm32 compilation bug (references `keepalive_config`) |
| `posix`       | Requires POSIX APIs |
| `shmop`       | Requires POSIX shared memory |
| `sysvmsg`     | POSIX message queues (`libc::IPC_NOWAIT` unavailable on WASI) |
| `sysvsem`     | Requires POSIX semaphores |
| `sysvshm`     | Requires POSIX shared memory |

## Performance branches

Run `just perf-pr-guard` before proposing any performance-labeled change.
The guard fails measurement theater: docs/report/counter-only diffs, hot-path
edits without gates, and native/JIT reporting while no native code changed.
A performance claim needs production Rust changes plus an executable gate
(`just profiler-overhead-gate`, the WordPress root benchmark, or a focused
fixture with before/after counters).
