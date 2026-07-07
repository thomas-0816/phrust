# Runtime semantics Runtime Semantics Contract

Runtime semantics extends the Runtime executable core into a more PHP-compatible
runtime-semantics layer. It keeps the existing frontend and execution pipeline:

```text
SourceText
  -> php_lexer
  -> php_syntax
  -> php_ast
  -> php_semantics / HIR
  -> php_ir
  -> php_runtime
  -> php_vm
  -> php_vm_cli
```

Runtime semantics must not add a second lexer, parser, AST, semantic frontend, or source
string-matching path. Runtime work consumes Semantic frontend HIR and Runtime IR/VM
metadata.

## Reference Target

- PHP series: `8.5`
- PHP version: `8.5.7`
- Git tag: `php-8.5.7`
- Repository: `https://github.com/php/php-src.git`
- Preferred runtime oracle: `third_party/php-src/sapi/cli/php`

Reference-dependent checks must skip clearly when no PHP reference binary is
available. If `REFERENCE_PHP` is explicitly set, failures to execute it are
strict validation failures.

## Scope

Runtime semantics focuses on language runtime semantics that were explicit Runtime gaps:

- References: local aliases, by-reference parameters, by-reference returns,
  array-element references, object-property references, closure uses, `global`,
  `$GLOBALS`, and `foreach (&$value)`.
- Copy-on-Write: string and array sharing, separation for write, interaction
  with references, lvalues, temporaries, and foreach.
- Arrays: key normalization, packed/mixed storage transitions, insertion order,
  element lvalues, nested dimensions, unset, iteration, and mutation during
  iteration.
- Calls: named arguments, unpacking, variadics, default values, callable
  resolution, first-class callables, pipe operator, by-reference binding, and
  strict/coercive type checks.
- Objects: class entries, inheritance, visibility, static members, typed,
  uninitialized, readonly, dynamic, and hooked properties, late static binding,
  magic methods, cloning, clone-with, traits, interfaces, abstract/final checks,
  enums, attributes, and reflection metadata.
- Generators and fibers: real runtime objects and VM control flow instead of
  only known-gap diagnostics.
- Include, require, eval, and autoload: execution through the same frontend and
  VM pipeline, source mapping, scope rules, once semantics, and deterministic
  errors.
- Runtime lifetime: errors, exceptions, warnings, destructors, refcount/GC
  metadata, cycle collection, and shutdown ordering where required by visible
  language behavior.

## Non-Goals

Runtime semantics does not implement:

- A complete PHP standard library.
- A complete SPL implementation.
- Zend extension ABI compatibility.
- FPM, FastCGI, or other SAPI layers.
- Opcache, quickening, inline caches, bytecode cache, or JIT.
- Zend bytecode compatibility.
- Performance parity claims against Zend.

Small internal builtins and predefined classes/interfaces are allowed only when
they are necessary to make language semantics, reflection, generators, fibers,
enums, attributes, or autoloading testable.

Standard-library builtin dispatch is governed by
[`stdlib-abi-dispatch.md`](../stdlib/abi-dispatch.md). New PHP standard-library
functions should use the registry-backed `InternalRegistry` path unless they
need VM-only services documented there.

VM public API and module ownership are governed by
[`api-facades.md`](../api-facades.md). Downstream crates should use
`php_vm::api` for execution-facing types and reserve `php_vm::experimental` for
performance tooling or VM-internal instrumentation.

## Semantics Priorities

1. References, slots, Copy-on-Write, array element lvalues, and foreach.
2. Function, closure, callable binding, by-reference parameters/returns, and
   strict/coercive type checks.
3. Object model, visibility, properties, magic methods, clone/clone-with, and
   late static binding.
4. Traits, interfaces, enums, attributes, and reflection MVP.
5. Generators and fibers.
6. Include, require, eval, autoload, globals, and real-world smoke fixtures.
7. GC, destructors, failure minimization, tracing, and hardening.

If implementation order changes because the current Runtime architecture
requires a prerequisite first, the reason must be documented in the relevant
commit or docs update.

## GC Root Tracking And Debug Metadata

Work item adds detection-only GC metadata under the `php_runtime::debug` facade
backed by the internal `php_runtime::gc` module. Runtime values
can be scanned from explicit `GcRoot` entries into deterministic `GcSnapshot`
nodes for arrays, objects, references, closures, and reserved generator/fiber
categories. The VM-owned root helper covers frame registers, frame locals,
static locals, static properties, enum-case objects, and destructor queue
entries. Generator/fiber suspended stack roots are reserved but empty until
those runtime objects are implemented.

The debug scanner records Rust `Rc` strong-count estimates where available and
marks entities that can reach themselves as cycle candidates. It does not
collect, free, run destructors, or expose PHP-visible handles. Full cycle
collection and exact refcount-triggered lifetime remain later Runtime semantics
work and known gaps.

Work item extends this with internal weak handles plus `GcTrackedHeap`, a test
hook that can clear unrooted object properties and reference cells to break
simple cycles. This hook is deterministic, does not execute PHP code, and does
not own destructor scheduling. Public `gc_collect_cycles()`, `gc_status()`,
WeakReference, WeakMap, Zend-compatible collection counts, and cyclic
destructor timing remain known gaps unless a later work item implements them.

## Generator MVP

Work item replaces the blanket generator lowering gap for free functions with
an internal generator architecture. Functions whose Semantic frontend signature contains
`yield` are marked in IR, and a normal call returns a `Generator` runtime value
without executing the body. By-value `foreach` over that value runs the body
until the first `yield`, exposes the yielded key/value to the loop, and records
Created, Running, Suspended, Closed, and Errored state transitions.

Work item exposes the visible `Generator` method surface for the language MVP.
`current()`, `key()`, `valid()`, `rewind()`, `next()`, `send()`, `throw()`,
and `getReturn()` operate on the same `GeneratorRef` state as `foreach`. The VM
stores suspended generator continuations with the saved frame, block,
instruction offset, foreach state, exception handlers, and pending finally
control. `send()` writes the sent value back to the suspended `yield`
expression. `throw()` injects an internal throwable object through the saved
handler stack. `getReturn()` is available after normal completion and reports a
deterministic runtime error before completion.

Work item extends generator continuations with `yield from` delegation for
arrays and generator MVP objects. Array delegation preserves insertion-order
keys and values. Generator delegation forwards yielded keys/values, resumes the
delegate until completion, and makes the delegate return value available as the
`yield from` expression result. `foreach` over generators uses the same stateful
resume path, including return-value completion. By-reference generator yields
are detected through by-reference generator declarations and reported as
`E_PHP_RUNTIME_GENERATOR_BY_REF_YIELD_GAP` until yielded reference cells are
represented explicitly.

Remaining generator boundaries are Iterator/SPL object delegation beyond arrays
and generator MVP objects, by-reference yield, generator methods/closures beyond
the free-function MVP, full engine diagnostic text, and the wider
exception/finally/destructor interaction matrix.

## Object Iteration and Iterator MVP

Work item extends `foreach` beyond arrays for the language-level object cases
needed before a full SPL implementation. Plain objects iterate their visible
public instance properties without array conversion; the VM rereads the
property list/value on each step so fixture-covered mutations to not-yet-read
properties are visible.

Objects implementing the internal `Iterator` metadata dispatch through
`rewind`, then repeat `valid`, `current`, optional `key`, and `next` in PHP
iteration order. Objects implementing `IteratorAggregate` dispatch
`getIterator()` and then recursively iterate the returned array, generator MVP
object, public-property object, or `Iterator` object. This is intentionally a
language dispatch MVP, not a complete SPL compatibility layer.

`ArrayAccess` metadata is available for class declarations, but object offset
indexing remains a standard library SPL gap. Full SPL method signatures, built-in
iterator classes, mutation edge matrices, exact diagnostic text, and
Iterator/IteratorAggregate behavior that depends on extension classes remain
out of Runtime semantics.

## Serialization Magic Boundary

Work item keeps serialization out of Runtime semantics execution while making the gap
deterministic. The builtin registry recognizes `serialize`, `unserialize`, and
`var_export`, but each returns `E_PHP_RUNTIME_SERIALIZATION_STDLIB_GAP` instead
of producing partial output or skipping magic hooks. Runtime method metadata
preserves `__serialize`, `__unserialize`, `__sleep`, and `__wakeup` names for
reflection, diagnostics, and the standard library follow-up.

standard library must decide the complete standard-library serialization model,
including object property encoding, `__serialize`/`__unserialize`,
`__sleep`/`__wakeup`, enum serialization rules, allowed-class options,
reference identity, cyclic structures, `Serializable` compatibility, and
`var_export`/`__set_state` reconstruction. Until that exists, serialization
fixtures are known gaps and must not produce plausible but incorrect results.

## PHP 8.5 Runtime Status

Work item keeps the PHP 8.5 runtime surface explicit:

- Pipe operator execution is covered for user functions, builtins, closures,
  dynamic string callables, invokable objects, and chains through the unified
  callable path. Non-callable RHS values produce `E_PHP_VM_PIPE_RHS_NOT_CALLABLE`.
- `(void)` cast is accepted syntactically by the parser fixtures but rejected by
  the pinned semantic/reference boundary as `E_PHP_INVALID_VOID_CAST`; Runtime semantics
  does not expose a fake expression value for it.
- Clone-with remains the Work item/24 public-property MVP. Private/protected,
  readonly, static, property-hook-complete, and reference-property replacement
  rules stay specific known gaps.
- PHP 8.5 scalar cast parameter defaults are folded into IR constants for
  fixture-covered scalar values. Constant-expression callables, closures, and
  `new` forms are recorded by the semantic frontend, but Runtime semantics does
  not materialize their runtime default values yet; unsupported forms stay
  under `E_PHP_RUNTIME_CONST_EXPR_MATRIX`.

## Fiber MVP

Work item introduces real internal `Fiber` runtime objects. `new Fiber(...)`
stores a callable in `php_runtime::FiberRef` with explicit NotStarted, Running,
Suspended, Terminated, and Errored lifecycle states. The VM recognizes
`Value::Fiber` as object-like for type checks, `instanceof Fiber`, `gettype`,
`var_dump`, conversions, and GC root scanning of the stored callable.

The MVP `start()` path executes the callable through fiber-aware VM callable
dispatch and records normal completion as Terminated or failed execution as
Errored. `isStarted()`, `isSuspended()`, `isRunning()`, and `isTerminated()`
report the stored state. Invalid construction and invalid start ordering
produce deterministic FiberError-class runtime diagnostics.

Work item adds cooperative fiber stack switching for the fixture-covered VM
subset. Static `Fiber::suspend()` saves the active frame plus any caller frames
waiting on nested function/static/method/closure calls. `start()` and
`resume()` return the suspended value, or `null` when the fiber terminates
without suspending again. `resume($value)` writes the value back into the
suspended `Fiber::suspend()` expression. `throw($exception)` injects a
Throwable through the saved handler stack. `getReturn()` exposes the callable
return value after normal termination and reports deterministic FiberError-class
diagnostics before termination or after an errored fiber.

Remaining fiber boundaries are wider stack switching across include/magic/hook
edges, catchable FiberError objects for VM method failures, public
`Fiber::getCurrent()`, exact engine diagnostic text, public GC/refcount timing
around suspended stacks, and destructor/generator/fiber interaction matrices.

## Include/Require MVP

Work item makes `include`, `require`, `include_once`, and `require_once`
executable for local root-constrained files. Included source is loaded through
the existing `php_semantics`/`php_ir`/`php_vm` pipeline with per-file source
paths and source text; it does not add a second lexer, parser, or frontend path.

Included top-level code shares the caller frame's local variable scope for the
fixture-covered cases, writes modified locals back to the caller, and returns
the included file's `return` value to the include expression. Relative includes
inside an included file search configured `include_path` entries first, then
the including file directory, the request current working directory, and the raw
relative path. An `include_path` entry of `.` is resolved to the including file
directory when one is active, otherwise to the request current working
directory. Absolute paths bypass relative candidate search. Every local
filesystem candidate is canonicalized and must remain below a configured
allowed root. `phar://` includes remain supported for local archives permitted
by the same filesystem capability roots.

Once semantics canonicalize paths through the include loader and are stable
within a VM request context, so `include_once` and `require_once` skip files
already loaded by either once form. The process-local include cache stores only
path-resolution metadata and compiled include units keyed by canonical path,
file fingerprint, compiler configuration, and optimization level; request-local
locals, globals, symbol tables, include-once state, autoload registry state, and
call-site strictness are never cache payloads. Modified include files invalidate
stale cache entries through metadata fingerprints. Poisoned cache locks surface
as deterministic `E_PHP_VM_INCLUDE_CACHE_POISONED` VM errors.

Missing `require` remains fatal and missing `include` continues after emitting a
structured warning diagnostic. Exact PHP warning stream text and channel
placement remain covered by `E_PHP_RUNTIME_WARNING_CHANNEL_COMPAT`.

## Eval MVP

Work item lowers `eval($code)` to a runtime VM instruction whose evaluated
string is compiled through the same `php_lexer` -> `php_syntax` ->
`php_semantics` -> `php_ir` -> `php_vm` pipeline as normal source and includes.
The VM wraps the evaluated code in a synthetic PHP source file with `eval://N`
display paths and executes it in the current request state.

Eval code shares the caller's top-level local variable scope for the
fixture-covered cases, writes modified locals back on successful completion,
and returns the evaluated code's `return` value to the eval expression. Eval
parse and compile failures emit `E_PHP_VM_EVAL_PARSE_ERROR` or
`E_PHP_VM_EVAL_COMPILE_ERROR` with synthetic source context. Nested eval uses
the same execution state and is bounded by `E_PHP_VM_EVAL_RECURSION_LIMIT` to
avoid recursive VM panics.

Eval-time named function, class, and constant declarations merge into the
request-local runtime symbol tables for fixture-covered top-level eval code,
are available to later runtime lookups, can participate in simple inheritance
relationships, and remain visible to the autoload lookup that triggered
eval-generated declarations. Duplicate eval-declared functions and classes are
rejected deterministically before they can silently override earlier
declarations. Conditional function declarations from include/eval units are
execution-time side effects for the covered fixtures and are not pre-registered
statically. Duplicate global `const` declarations emit PHP 8.5-compatible
warnings, preserve the first value, and continue for the covered fixtures. Exact
`ParseError` object parity plus wider eval scope interactions remain later work.

## Autoload MVP

Work item adds a request-local `AutoloadRegistry` and VM-owned handlers for
`spl_autoload_register`, `spl_autoload_unregister`,
`spl_autoload_functions`, `spl_autoload_call`, `class_exists`, and
`interface_exists` in the Runtime semantics MVP scope. The registry accepts Runtime semantics
structured callables: closures, string/user-function callables, and internal
builtin callables that the VM can already invoke.

Autoload is attempted when `new UnknownClass` reaches an unresolved class name
and when `class_exists` or `interface_exists` requests autoloading. It is not
run for every name comparison or metadata query. A normalized class-name guard
stack prevents recursive autoload loops; recursive probes for the same class
return without re-entering the callback chain.

Include-based autoload callbacks can load source files through the same include
pipeline. Source-backed class and interface metadata from included files is
registered in a VM dynamic class table for fixture-covered existence checks and
simple no-constructor object creation. Autoload callbacks that `eval` class
declarations register those declarations in the same request-local dynamic
symbol tables. Full Composer compatibility is not claimed. Standard-library
coverage still needs cross-unit symbol linking for methods, properties with
nontrivial defaults, constants, inheritance, trait/enum details, PSR-0/PSR-4
path rules, and complete SPL error/object parity.

## Globals and Superglobals MVP

Work item adds a VM-owned global symbol table for request execution. Top-level
locals are bound to global slots with `ReferenceCell` storage, and `global $x`
inside functions or closures binds the function-local slot to the same global
cell instead of copying the value. Plain function-local assignments without a
`global` statement remain local to that function.

`$GLOBALS` is exposed as a live view over the global symbol table for the
fixture-covered direct dimension cases. Reads such as `$GLOBALS["x"]` observe
the current global slot, and writes such as `$GLOBALS["x"] = $value` update the
same slot visible through `$x` and `global $x`. Nested writes through an
existing `$GLOBALS["x"][...]` array value are supported by routing the nested
write through the global slot. Appending directly to `$GLOBALS` and dynamic
global-variable declarations remain outside the Runtime semantics MVP.

The CLI runtime context seeds `$argc`, `$argv`, `$_SERVER`, `$_ENV`, and empty
request superglobals (`$_GET`, `$_POST`, `$_COOKIE`, `$_FILES`, `$_REQUEST`) in
a deterministic way. `$_SERVER` currently documents the fixture-supported
`argc` and `argv` entries only; host environment and SAPI-specific server keys
are not imported implicitly.

Include and eval continue to execute through the existing frontend/IR/VM
pipeline. In top-level caller scope, their shared-local import/export writes
through the same global slots. In function or closure caller scope, include and
eval share the caller locals for the fixture-covered cases; any `global`
statement inside that code still binds to the request global table.

## Follow-up From Runtime

Runtime established:

- HIR-to-IR lowering in `php_ir`.
- Interpreter execution in `php_vm`.
- Minimal runtime values in `php_runtime`.
- CLI execution and reporting through `php_vm_cli`.
- Runtime fixture comparison and PHPT smoke infrastructure in `php_testkit`.
- A Runtime known-gap catalog in `docs/runtime/known-gaps.md`.

Work item baseline validation was run before Runtime semantics docs edits:

```bash
nix develop -c just verify-runtime
nix develop -c cargo test --workspace
```

Both commands passed on the unchanged Runtime worktree. The `verify-runtime`
gate reports that `runtime-diff` remains reference-gated outside
`verify-runtime`; Runtime semantics differential work must continue to respect that
boundary until `verify-runtime` defines the stricter Runtime semantics gate.

## Diagnostic Policy

Runtime semantics must prefer specific diagnostics over silent wrong output. Diagnostic
families reserved for Runtime semantics include:

```text
E_PHP_RUNTIME_REF_*
E_PHP_RUNTIME_COW_*
E_PHP_RUNTIME_ARRAY_*
E_PHP_RUNTIME_CALL_*
E_PHP_RUNTIME_OBJECT_*
E_PHP_RUNTIME_MAGIC_*
E_PHP_RUNTIME_TYPE_*
E_PHP_RUNTIME_GENERATOR_*
E_PHP_RUNTIME_FIBER_*
E_PHP_RUNTIME_INCLUDE_*
E_PHP_RUNTIME_EVAL_*
E_PHP_RUNTIME_AUTOLOAD_*
E_PHP_RUNTIME_REFLECTION_*
W_PHP_RUNTIME_GAP_*
```

Existing Runtime IDs remain valid until their feature is implemented or split
into more specific Runtime semantics IDs. Every new unsupported language-semantics gap
needs a stable ID, fixture, and entry in `docs/runtime/semantics-known-gaps.md`.

## Runtime semantics Commands

Work item adds the central Runtime semantics gate and initial fixture categories:

```bash
nix develop -c just verify-runtime
nix develop -c just runtime-semantics-fixtures
nix develop -c just runtime-semantics-diff
nix develop -c just refs-cow-fixtures
nix develop -c just object-semantics-fixtures
nix develop -c just generator-fiber-fixtures
nix develop -c just real-world-fixtures
nix develop -c just regression-fixtures
nix develop -c just runtime-phpt-smoke
```

`runtime-phpt-smoke` is driven by
`fixtures/runtime_semantics/phpt_allowlist.toml`. The allowlist references a small curated
set of pinned `php-src` PHPT files instead of vendoring the upstream test tree.
Each entry carries a Runtime semantics category plus a disposition:

- `run`: execute and compare the PHPT expectation against `php-vm`.
- `expected_fail`: execute and require the current Runtime semantics gap to remain
  visible in the report.
- `skip` or `known_gap`: classify without execution and require a reason.

The current matrix covers language smoke, references/COW, foreach, traits,
enums, generators, fibers, property hooks, and reflection. `verify-runtime`
runs `fmt`, `lint`, workspace tests, and `verify-runtime` before Runtime semantics fixture,
diff, and PHPT gates, so the Runtime baseline remains protected.

`fixtures/runtime_semantics/real_world/` contains offline, hand-written Composer-like
smokes. These fixtures do not download packages, do not vendor `vendor/`, and
do not require Composer. The required `real-world-fixtures` gate covers a
self-contained service that combines reflection, attributes, traits, enum type
checks, and closure captures, plus explicit known-gap fixtures for Composer
autoload and framework-container stdlib/reflection breadth.

Local Composer or framework project experiments are opt-in:

```bash
nix develop -c just local-composer-smoke path/to/local/project
nix develop -c just runtime-composer-smoke
```

That target is not part of `verify-runtime`; it is for user-provided local paths
only and must not introduce network downloads or committed vendor trees.
`runtime-composer-smoke` is the environment-driven variant: it skips unless
`PHPRUST_COMPOSER_FIXTURE_DIR` points at an existing local project and writes
its normalized gap report under `target/runtime-semantics/composer-smoke`.

Optional local stress probes are also available:

```bash
nix develop -c just runtime-fuzz-smoke
nix develop -c just runtime-bench-smoke
```

`runtime-fuzz-smoke` generates a deterministic, bounded set of small programs
for references, Copy-on-Write arrays, `unset`, and `foreach`, compares them
against `REFERENCE_PHP` when the pinned reference binary exists, and stores
minimization inputs under `target/runtime-semantics/fuzz-smoke`. It does not commit
generated regressions unless `--save-regressions` is passed directly.
`runtime-bench-smoke` records local smoke timings for arrays, calls, objects,
generators, and fibers under `target/runtime-semantics/bench-smoke`; these timings are
for regression spotting only and are not PHP/Zend benchmark claims.

## Failure Minimization and Regressions

Work item adds a documented path from a Runtime semantics diff failure to a permanent,
small regression fixture:

1. Reproduce the mismatch with a focused diff command, for example
   `nix develop -c env REFERENCE_PHP=third_party/php-src/sapi/cli/php just runtime-semantics-diff --file path/to/failure.php`.
2. Minimize the fixture against both engines:
   `nix develop -c env REFERENCE_PHP=third_party/php-src/sapi/cli/php python scripts/minimize_runtime_failure.py path/to/failure.php --out target/runtime-semantics/minimized.php`.
3. Inspect the minimized file and move only the minimal PHP source into
   `fixtures/runtime_semantics/regressions/pass/` if it now passes, or
   `fixtures/runtime_semantics/regressions/known_gaps/` if it must remain an explicit
   known gap.
4. Add the required inline metadata on the first lines:
   `expect=...`, `regression_category=...`, `reference_behavior=...`, and
   `regression_case=...`; known-gap regressions must also include `known_gap=...`.
5. Run `nix develop -c just regression-fixtures`, then the work item-specific gate.

Regression fixtures must stay small, handwritten PHP files. They must not be
large framework snapshots, vendored Composer trees, or generated reports.
`regression-fixtures` is included by `runtime-semantics-fixtures`, so `verify-runtime`
includes the regression corpus through the central Runtime semantics gate.

## Core Documentation and ADRs

Runtime semantics behavior is split across these topic documents:

- `docs/runtime/semantics-reference-cow.md` for `Slot`, `ReferenceCell`, temporaries, and
  Copy-on-Write invariants;
- `docs/runtime/state-ownership.md` for `ExecutionState`, `CallStack`,
  `Frame`, local/register storage, request-global roots, and frame-pool
  invariants;
- `docs/api-facades.md` for the stable VM execution API, experimental VM
  instrumentation facade, and module ownership map;
- `docs/runtime/semantics-array-semantics.md` and `docs/runtime/semantics-foreach-semantics.md` for
  key normalization, append behavior, element references, foreach snapshots,
  and object/Iterator iteration;
- `docs/runtime/semantics-object-semantics.md` for class metadata, visibility,
  properties, traits, interfaces, enums, hooks, magic methods, cloning, and
  destructors;
- `docs/runtime/semantics-generators-fibers.md` for generator and fiber runtime objects
  and VM continuation boundaries;
- `docs/runtime/semantics-reflection-attributes.md` for attribute metadata and Reflection
  metadata handles;
- `docs/runtime/semantics-known-gaps.md` for every unsupported or deferred runtime
  behavior that remains visible to users or tests;
- `docs/runtime/semantics-hardening.md` for hardening, unsafe-code, Miri, and
  sanitizer status;
- `docs/runtime/semantics-coverage-matrix.md` and `docs/runtime/semantics-validation.md` for the
  final Runtime semantics gate and coverage closure state;
- `docs/runtime/semantics-status.md` for the Runtime-to-Runtime semantics working follow-up record;
- `docs/stdlib/roadmap.md` for the concrete standard library backlog.

Architecture decisions are recorded in:

- `docs/runtime/semantics-contract.md`;
- `docs/runtime/semantics-contract.md`;
- `docs/runtime/semantics-reference-cow.md`;
- `docs/runtime/semantics-array-semantics.md`;
- `docs/runtime/semantics-object-semantics.md`;
- `docs/runtime/semantics-generators-fibers.md`.

## Runtime Tracing

Work item adds opt-in runtime tracing for minimizing Runtime semantics failures without
changing PHP-visible output. Instruction tracing remains available with
`php-vm run --trace`. Runtime-state tracing is enabled separately:

```bash
nix develop -c cargo run -p php_vm_cli -- run --trace-runtime fixtures/runtime_semantics/arrays/array-element-reference-write-through.php 2> trace.log
```

Trace lines are written to stderr under the existing `vm-trace:` header. Normal
PHP stdout is written only to stdout, so trace collection must not change
runtime semantics or fixture comparisons.

Runtime trace events currently cover:

- lvalue, reference, COW-facing array write/append/unset, and array-element
  reference binding operations;
- foreach initialization and next/done transitions for by-value and
  by-reference iteration;
- object method dispatch after visibility and method resolution;
- generator Created/Running/Suspended/Closed/Errored transitions, suspend
  keys/values, and resume input shape;
- fiber start, suspend, resume, throw, termination, and error transitions;
- VM GC-root summary counts after execution.

Trace formatting redacts process-local or nondeterministic identities. Object,
fiber, generator, reference, and GC entity handles are represented by stable
classes, states, counts, values, or `<redacted>`-free summaries instead of raw
addresses or pointer-derived IDs. Snapshot tests assert that traces do not leak
`0x...` addresses or raw `id=` fields.

## standard library Public API Surface

standard library should consume the existing crates through these public APIs instead of
adding parallel representations:

- VM execution: `php_vm::Vm`, `VmOptions`, and `VmResult`. `VmOptions` carries
  IR verification, step limits, include loading, runtime context, instruction
  tracing, and runtime tracing. `VmResult` carries status, output,
  diagnostics, return value, and optional trace output.
- Source loading: `php_vm::IncludeLoader` and `LoadedInclude` define the
  root-constrained include/eval/autoload file boundary.
- Compiled metadata: `php_vm::CompiledUnit` exposes function, constant, and
  class tables derived from `php_ir::IrUnit`.
- Runtime values: `php_runtime::Value`, `FloatValue`, `PhpString`,
  `PhpArray`, `ArrayKey`, `ObjectRef`, `CallableValue`, `ClosureCaptureValue`,
  `GeneratorRef`, and `FiberRef`.
- Storage and references: `php_runtime::api::Slot`, `ReferenceCell`,
  `TempValue`, and `ValueSlot`; weak debug handles for arrays/references/objects
  are available only through `php_runtime::debug` or compatibility root aliases.
- Class and reflection metadata: `ClassEntry`, method/property/constant/enum
  entries and flags, `RuntimeType`, `AttributeEntry`, `runtime_type_name()`,
  and `value_matches_runtime_type()`.
- Request services: `RuntimeContext`, `GlobalSymbolTable`, `AutoloadRegistry`,
  `BuiltinRegistry`, `BuiltinContext`, `OutputBuffer`, and structured
  `RuntimeDiagnostic` / `RuntimeError` values.
- GC/debugging: `php_runtime::debug::{GcRoot, GcRootKind, GcSnapshot,
  scan_roots, GcTrackedHeap}` remain debug/test APIs until public `gc_*`
  semantics are implemented.

These APIs are public Rust surfaces, but not all are compatibility promises.
Frame/register internals, VM continuation structs, GC debug IDs, and trace
format details may change when standard library adds broader standard-library,
Composer, and performance work. User-visible PHP semantics must stay covered by
fixtures and known-gap diagnostics before an API is treated as stable.

The current frame/register reuse pool is an internal request-local optimization.
Only reuse-eligible plain user-function activations enter the pool. Generator and
fiber continuations keep owning their saved frames, and conservative fallback
uses fresh frames for closure captures, by-reference calls/returns, class
contexts, shared top-level locals, try/finally bodies, and object-allocation
bodies that may retain destructor-sensitive values. These rules are observable
only through VM counters; they must not alter PHP-visible output, diagnostics,
side-effect order, references, or destructor timing. FPE-19 adds
request-arena-shaped counters for this existing pool, but does not add bulk
arena reset for values, objects, arrays, references, resources, generators,
fibers, output buffers, or any userland state.

## performance-critical Boundaries

Runtime semantics intentionally favors determinism and explicit diagnostics over Zend
performance parity. The main areas likely to need standard library optimization are:

- array storage transitions, COW separation, and reference-cell writes on hot
  foreach/append paths;
- object method/property lookup, hook dispatch, magic recursion guards, and
  Reflection metadata lookups;
- generator and fiber continuation cloning;
- include/eval/autoload compilation reuse and cross-file symbol linking;
- builtin dispatch, callable resolution, numeric-string classification, and
  type coercion matrices;
- GC root scanning and destructor queue shutdown ordering.

None of these performance notes authorize silent semantic shortcuts. If an
optimization changes observable output, side-effect order, diagnostic class, or
known-gap behavior, it needs a fixture and an updated ADR or topic document.

## Hardening and Unsafe Audit

Runtime-semantics hardening is tracked in `docs/runtime/semantics-hardening.md`.
`verify-runtime` includes:

- `just runtime-hardening-lints`, which runs Clippy for `php_runtime` and
  `php_vm` with `-D warnings -D unsafe-code`;
- `just runtime-toolchain-audit`, which checks that the Nix devshell exposes the
  required local tools and pinned PHP reference metadata.

Additional opt-in hardening targets are documented but are not part of the
required gate:

- `nix develop -c just runtime-miri-smoke`;
- `nix develop -c just runtime-sanitizer-smoke`.

They skip cleanly when the active toolchain cannot support them. CI must not
depend on external Rust component downloads or local user toolchains for these
optional checks.

## Validation Direction

Each Runtime semantics work item must validate its own fixture category before the next
work item begins. Differential fixtures should compare visible stdout, stderr
category, exit status, exception/error class, and side-effect ordering against
`REFERENCE_PHP`; path and line normalization is allowed, but visible runtime
values must not be normalized away.
