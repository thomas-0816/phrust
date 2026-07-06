# zend.functions Implementation Notes

Status: research note, not an accepted project contract.

Current focused `zend.functions` status is tracked in
`target/phpt-work/reports/zend.functions-current.md`. This document keeps the
implementation notes for functions, callables, arity, and type coercion.

> Line numbers drift as you edit `crates/php_vm/src/vm/mod.rs` and the
> `crates/php_ir/src/lower/` modules. Use the **symbol names** as the stable anchor and
> re-`grep` for current lines.

---

## 1. How to build, run, and measure

Everything runs in the Nix dev shell (host `cargo` also works on this macOS box).

```bash
# build the two CLIs used for PHPT
just phpt-dev-build                     # builds php-phpt-tools + phrust-php

# measure the module (target vs pinned PHP 8.5.7 reference)
rm -rf /private/tmp/phrust-phpt-work/module-runs/zend.functions
just phpt-dev-module MODULE=zend.functions      # prints "ran 200 ... with N non-green"

# run ONLY the generated zend.functions fixtures
PHPT_MANIFEST=tests/phpt/manifests/zend.functions-generated.jsonl \
  just phpt-dev-module MODULE=zend.functions

# developer VM CLI for ad-hoc repros
./target/debug/php-vm run /tmp/x.php
# reference oracle (PHP 8.5.7, must be this exact version)
./third_party/php-src/sapi/cli/php -n /tmp/x.php

# per-test primary blocker tally (very useful)
jq -r 'select(.outcome=="FAIL") | .detail' \
  /private/tmp/phrust-phpt-work/module-runs/zend.functions/target/results.jsonl \
  | while IFS= read -r l; do echo "$l" | grep -oE "E_PHP_[A-Z_]+" | head -1; done \
  | sort | uniq -c | sort -rn
```

Acceptance gates: `just verify-runtime`, `just verify-stdlib`,
`just phpt-module MODULE=zend.functions`, and the full refresh
`REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php PHPT_RUN_FULL=1 just phpt-full-regression`.

Per-crate quick loops while iterating:
`cargo test -p php_vm`, `cargo test -p php_ir`, `cargo clippy -p php_vm -- -D warnings`.

---

## 2. What is already done (build on this — do not redo)

The exception/error subsystem and a chunk of callable/arity/coercion behavior
landed this session. Relevant commits: `a1e60370`..`7ac08c17`.

- **Cross-frame exception propagation** — a throw inside a called function now
  unwinds to an enclosing `try/catch`. Carrier is `ExecutionState::pending_throw`;
  each caller re-throws via its handlers; only the entry point / shutdown renders
  uncaught. See `propagate_exception`, `handle_throw`, entry point in
  `Vm::execute`.
- **Stack-trace capture** — `capture_backtrace_string` snapshots frames at the
  throw origin into `ExecutionState::pending_trace`; `uncaught_exception` renders
  it. Frame call-site **line numbers are placeholders** (function def line, not
  the call site) — EXPECTF `%s(%d)` tolerates it, but EXACT `--EXPECT--` trace
  tests will still mismatch. Fixing this needs call-site-line plumbing (see §4).
- **Catchable/uncaught PHP `Error`/`TypeError` rendering** for: private/protected/
  abstract method access, private/protected property reads, private/protected
  class constants, undeclared static property, method-call-on-non-object,
  non-static-method-called-statically, **argument type mismatch → `TypeError`**.
  All routed through the shared helper `raise_runtime_error` (see §3).
- **First-class callables from methods** — `$o->m(...)`, `Cls::m(...)` lower to
  `[recv,'m']` array callables (`lower_method_first_class_callable` in
  `crates/php_ir/src/lower/expressions.rs`). Plain `f(...)` already worked.
- **Extra positional args to user functions** are accepted and visible to
  `func_get_args()` (was wrongly an `ArgumentCountError`). See `prepare_arguments`.
- **Static method via instance** `$obj->staticMethod()` no longer wrongly rejected.
- Regression PHPTs in `tests/phpt/generated/zend.functions/` (10 total, all green):
  `builtin-too-few-args`, `builtin-too-many-args`, `by-ref-mismatch`,
  `weak-coercion`, `strict-types-builtin`, `variadic-packing`,
  `callable-invocation`, `is-callable-forms`, `user-extra-args`,
  `first-class-callable-methods`. Manifest:
  `tests/phpt/manifests/zend.functions-generated.jsonl`.

Gate status: `verify-runtime`, `verify-stdlib`, `verify-phpt` all green; no
regressions in greener modules (zend.basic 0, standard.strings 1, standard.arrays 12).

---

## 3. Key patterns to reuse (the "how" for the VM layer)

All in `crates/php_vm/src/vm/mod.rs` unless noted:

- **`runtime_error_throwable(result) -> Option<Value>`** maps an error-id prefix
  to a throwable class (`E_PHP_VM_PARAM_TYPE_MISMATCH → TypeError`, the access
  errors → `Error`, etc.). To make a NEW detected error catchable, add its id to
  this match.
- **`raise_runtime_error(compiled, output, stack, state, handlers, pending_control, span, message) -> RaiseOutcome`**
  is the one-call helper: builds the error, tags location, captures the trace,
  routes through local handlers. Call sites do:
  ```rust
  match self.raise_runtime_error(compiled, output, stack, state,
        &mut exception_handlers, &mut pending_control, instruction.span, message) {
      RaiseOutcome::Caught(target) => { block_id = target; continue 'dispatch; }
      RaiseOutcome::Done(result) => return *result,
  }
  ```
  Use this ONLY at sites directly in the `'dispatch` loop. Helper methods that
  return a `VmResult` must instead return a `runtime_error(...)` whose first
  diagnostic id is mapped in `runtime_error_throwable`; the caller's call-opcode
  routing then re-throws it.
- **Message wording matters** — match PHP exactly (e.g. `Cannot access private
  property C::$p`, source-case class via `class.display_name` / `object.display_name()`,
  NOT the lowercased `class.name`).
- **Unit tests that assert old internal messages** must be updated when you change
  behavior (e.g. param-type/property-access tests assert
  `E_PHP_VM_UNCAUGHT_EXCEPTION` + output text now).

IR layer (`crates/php_ir/src/lower/expressions.rs`):
- `lower_callable_expr_to_register` / `lower_method_first_class_callable` — the
  callable-lowering entry points.
- Array build pattern: `InstructionKind::NewArray { dst }` then
  `InstructionKind::ArrayInsert { array, key, value, by_ref_local }`. Materialize a
  constant into a register with `emit_constant_to_register`.
- `method_call_target` / `static_method_call_target` extract receiver+method.

---

## 4. Remaining research blockers

Per-test PRIMARY blocker counts (current):

| # | Blocker | ~tests | Owning layer |
|---|---|---:|---|
| 1 | `E_PHP_IR_UNSUPPORTED_HIR_STATEMENT` | 62 | `php_ir` / `php_semantics` |
| 2 | `E_PHP_VM_UNKNOWN_CLASS` (26× `Closure`) | 34 | `php_runtime` / `php_vm` |
| 3 | `E_PHP_VM_INVALID_STATIC_SCOPE` | 20 | `php_vm` |
| 4 | `E_PHP_VM_UNINITIALIZED_PROPERTY` | 16 | `php_vm` |
| 5 | `E_PHP_VM_UNKNOWN_METHOD` | 10 | `php_vm` |
| 6 | `E_PHP_VM_PIPE_RHS_NOT_CALLABLE` (`\|>`) | 8 | `php_ir` / `php_vm` |
| 7 | `E_PHP_INVALID_CLASS_CONTEXT_NAME` | 7 | `php_vm` |
| 8 | `E_PHP_VM_INVALID_CALLABLE_ARRAY` | 6 | `php_vm` |
| 9 | `E_PHP_RUNTIME_UNDEFINED_FUNCTION` | 6 | builtins (arginfo) |
| 10 | `E_PHP_RUNTIME_CALLABLE_CONTEXT_REQUIRED` | 6 | `php_vm` |

### Cluster 1 — IR lowering (biggest, 62). Sub-kinds:
- **28 × "global const initializer is not a folded constant expression"** — these
  are almost all `Zend/tests/first_class_callable/constexpr/*` doing
  `const X = f(...)` (a first-class callable in a const expression). `php_ir`:
  `lower_global_constant_declarations` → `global_const_initializers` →
  `constant_from_expr` (in `crates/php_ir/src/lower/consts.rs`, handles only Literal/Name/Array). PHP 8.5 allows FCC in
  const-expr, producing a `Closure` constant. **Depends on the `Closure` class
  (cluster 2).** Lower priority unless you do `Closure` first.
- **12 × "isset only supports locals, properties, and local array dimensions"** —
  extend `isset` lowering to more expression forms (e.g. nested/dynamic dims,
  static props). Find the `isset` lowering in `lower.rs`.
- **8 × "only simple variable increment/decrement is lowered"** — compound
  `$obj->p++`, `self::$x++`, `$arr[k]++`. `lower.rs` ~`only simple variable
  increment/decrement is lowered to IR`. Needs load-modify-store lowering for
  property/static/dim targets.
- **5 × "new expression is missing its class operand"** — `new $class()` / dynamic
  `new`. `lower.rs` ~`new expression is missing its class operand`.
- **4 × parameter default not folded** — method/param defaults that aren't
  compile-time constants.

### Cluster 2 — `Closure` class (34, of which 26 are literally `class closure is not defined`)
This is the single biggest **self-contained** lever and unblocks part of cluster 1.
First-class callable *invocation* already works, but tests need the real
`Closure` object: `instanceof Closure`, `var_dump($fn)` → `object(Closure)`,
`Closure::fromCallable`, `Closure::bind`/`bindTo`, `$fn->call($obj)`, and FCC
yielding a `Closure` rather than the current array. Implement `Closure` as an
internal runtime class (see how throwables are modeled: `make_exception_object`
and `internal_throwable_*` in `crates/php_vm/src/vm/mod.rs` are the template for an internal class with
methods). Tests live under `third_party/php-src/Zend/tests/closures/` and
`.../first_class_callable/`.

### Cluster 3/4/7/10 — static-scope & property-init `Error`s (≈49)
`INVALID_STATIC_SCOPE`, `UNINITIALIZED_PROPERTY`, `INVALID_CLASS_CONTEXT_NAME`,
`CALLABLE_CONTEXT_REQUIRED`. Several of these are detected-but-dumped runtime
errors that PHP renders as catchable `Error` — the **exact same pattern** already
applied to method/property access. For each: fix the message to PHP wording, add
the id to `runtime_error_throwable`, and route the detection site through
`raise_runtime_error`. Verify each against the reference first — some may be
phrust wrongly rejecting valid code (like the static-method-as-instance bug that
was fixed this session), in which case the fix is to allow it, not to render an
error. `UNINITIALIZED_PROPERTY` → PHP: `Typed property C::$p must not be accessed
before initialization` (an `Error`).

### Cluster 6 — pipe operator `|>` (8)
PHP 8.5 `$x |> $fn`. `E_PHP_VM_PIPE_RHS_NOT_CALLABLE`. The FCC fix this session
made method callables resolvable; check whether pipe RHS now resolves for method
callables, then handle remaining callable forms. See `lower_pipe_to_register` in
`lower.rs` and the pipe handling in `vm.rs`.

### Cluster 9 — undefined builtins (6)
Builtins referenced by function tests that aren't generated from arginfo (e.g.
`array_sum` was observed missing). Builtins are generated from php-src
arginfo/stubs (`just generate-arginfo`, `just verify-stdlib`) — do NOT hand-write;
add the missing builtin via the arginfo pipeline + an override impl if needed.

### Cross-cutting: exact backtrace line numbers + `ArgumentCountError`/`TypeError` message location
`ArgumentCountError` and argument-`TypeError` need PHP's exact message with
`…called in FILE on line N and …expected in FILE:M`, and EXACT-trace tests need
real call-site lines. Both require **storing the call-site line on each `Frame` at
push time**. Add a field to `Frame` (`crates/php_vm/src/frame.rs`) set from the
calling opcode's `instruction.span`; thread it via `FunctionCall`. Then
`capture_backtrace_string` uses it (exact traces) and the arity/type errors can
include the caller location. ~9 `execute_function` call sites to thread through.

---

## 5. Repo non-negotiables (read before committing)

- **Commit messages**: conventional commits, imperative, < 72 chars first line.
  **Never** mention development provenance or development tooling.
- **Single pipeline** (`AGENTS.md`/`CLAUDE.md`): never add a second lexer/parser/
  AST/semantic frontend or a source-string-matching execution path. Fix bugs in
  the owning layer (lexer/parser → `php_lexer`/`php_syntax`; typed views/metadata
  → `php_ast`/`php_semantics`; lowering → `php_ir`; values/builtins →
  `php_runtime`/`php_std`; execution → `php_vm`).
- **Correctness = matching real PHP 8.5.7** at `third_party/php-src/sapi/cli/php`.
  Reference-dependent scripts must SKIP if php ≠ 8.5.7 and be strict when
  `REFERENCE_PHP` is set. Never edit `third_party/php-src/` (read-only oracle +
  PHPT corpus). Never commit `target/`.
- **Every behavior fix needs a focused regression fixture** (PHPT or minimized
  generated PHPT with provenance in the manifest).
- **Baselines are generated, never hand-edited.** The committed manifests in
  `tests/phpt/manifests/full-*.jsonl` + module docs are refreshed only by
  `PHPT_RUN_FULL=1 just phpt-full-regression`. New FAIL/BORK fingerprints are
  rejected unless `PHPT_ACCEPT_BASELINE=1` is explicit and justified — for this
  work you are REDUCING failures, so a refresh records improvements.
- Builtins come from arginfo (`just generate-arginfo`), not hand-written.
- `just verify-frontend` / `verify-runtime` / `verify-stdlib` / `verify-phpt` are
  the per-layer gates; run the one(s) for the layer you touched before closing
  the change.
  `clippy --workspace --all-targets -- -D warnings -D unsafe-code` must be clean.

---

## 6. Recommended order of attack

1. **`Closure` class** (cluster 2) — biggest self-contained win; also unblocks
   most of the 28 const-expr-FCC tests in cluster 1. Model it on the internal
   throwable class in `vm.rs`.
2. **Route the remaining detected `Error`s** (clusters 3/4/7/10) through
   `raise_runtime_error` — mechanical, reuses the established pattern, verify each
   against reference (some are wrong-rejections to allow, not errors to render).
3. **Frame call-site line plumbing** — unlocks exact backtraces +
   `ArgumentCountError`/`TypeError` message locations across the whole module.
4. **IR lowering** (cluster 1 remainder): compound `inc/dec`, complex `isset`,
   dynamic `new`. Each is a contained `lower.rs` change.
5. **Pipe operator** (cluster 6) and **missing builtins** (cluster 9).
6. After substantive greening: `PHPT_RUN_FULL=1 just phpt-full-regression` to
   refresh the committed baseline, then `just verify-phpt`.

Target acceptance: `zend.functions` green (or near-green with every remaining
failure recorded as a justified known-gap), all four focused gates passing, and
the "Am Ende" report (which callables work / which coercion rules work / which gaps
remain) updated.

---

## 7. Known-good repros (sanity checks; all currently match reference)

```php
$f = strlen(...);            echo $f("abcd");          // 5
$g = (new C)->m(...);        $g();                     // method FCC works
function one($a){return $a.'|'.implode(',',func_get_args());} echo one(1,2,3); // 1|1,2,3
declare(strict_types=1); function f(int $x){} f("42"); // Uncaught TypeError
$x=null; $x->foo();          // Uncaught Error: Call to a member function foo() on null
class C{protected $p=1;} echo (new C)->p;  // Uncaught Error: Cannot access protected property C::$p
```
