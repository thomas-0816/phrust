## Immediate review findings

The most urgent sanity issue: as fetched from GitHub, `crates/php_vm/src/vm.rs` is empty, while `crates/php_vm/src/lib.rs` still declares `pub mod vm` and re-exports `Vm`, `VmOptions`, `VmResult`, `JitMode`, `ExecutionFormat`, and related items from it. That has to be resolved before any new feature prompt proceeds.

Prompt 13 is currently overloaded. The prompt asks for user functions, closures, arrow functions, first-class callables, dynamic calls, `call_user_func`, defaults, variadics, by-ref params, return types, weak/strict coercion, and builtin arginfo in one pass.  The handover says the focused `zend.functions` state was only **14 PASS / 186 non-green out of 200**, with blockers spread across IR lowering, Closure runtime class, VM static-scope/property errors, pipe callables, undefined builtins, callable context, and backtrace/call-site line plumbing.

The broader module doc is even noisier: `zend.functions` is recorded as **85 PASS, 53 SKIP, 727 FAIL, 0 BORK from 887 corpus candidates**, with known gap clusters across runtime diagnostics, unsupported features, output mismatches, and frontend/compile failures.

`crates/php_runtime/src/object.rs` is already mixing too many ownership layers: runtime type metadata, class entries, flags, methods, properties, hooks, constants, enum cases, attributes, object storage, weak handles, `ObjectRef`, and broad tests for enum, reflection metadata, interface metadata, magic methods, and serialization magic all live in one file.

Builtin modularization exists structurally, but some modules are still registry shims pointing back into `core.rs`. For example, `filesystem.rs` registers filesystem functions but delegates all implementations to `core::builtin_*`; `math.rs` does the same.   That contradicts the documented intended ownership: new builtins should live in the module matching PHP module ownership, and shared helpers should only remain in `core.rs` when truly reused across modules.

`php_phpt_tools` has been partially split into `baseline`, `cli`, and `runner`, but `main.rs` still owns many unrelated responsibilities: constants, PHPT indexing, triage, module generation, symbol indexing, lookup, and option structs.

Arginfo is not ready enough for Prompt 13 to depend on it. `php_std::arginfo` imports `crate::generated::arginfo`, but the generated arginfo file fetched from GitHub is empty.

Below is a **linear prompt sequence**: first cleanup prompts, then rewritten Prompts 13–17 as smaller steps.

---

# Cleanup prompts before continuing Prompt 13

## Cleanup Prompt A — Restore VM module integrity

```text
You are working in `mayflower/phrust` on current `main`.

Goal:
Before adding new behavior, restore the VM module boundary so the workspace builds cleanly and `crates/php_vm/src/vm.rs` is not an empty module while `php_vm::lib` re-exports `Vm`, `VmOptions`, `VmResult`, `JitMode`, `ExecutionFormat`, and related items.

Context:
`crates/php_vm/src/lib.rs` exports VM API from `vm`, but `crates/php_vm/src/vm.rs` currently appears empty. Do not implement new PHP behavior in this prompt.

Tasks:
1. Inspect the last refactor commit and current `crates/php_vm/src/`.
2. Determine whether VM implementation was accidentally deleted, moved, or intended to be split.
3. Restore a compiling VM module structure.
4. If splitting was intended, introduce:
   - `crates/php_vm/src/vm/mod.rs`
   - `crates/php_vm/src/vm/options.rs`
   - `crates/php_vm/src/vm/result.rs`
   - `crates/php_vm/src/vm/executor.rs`
   - `crates/php_vm/src/vm/errors.rs`
   - `crates/php_vm/src/vm/calls.rs`
   - `crates/php_vm/src/vm/objects.rs`
   only as needed to preserve current behavior.
5. Keep the public exports from `php_vm::lib` stable.
6. Do not add new Zend/function/object behavior.
7. Update a short note in `docs/runtime-vm-structure.md`.

Acceptance:
- `nix develop -c cargo check -p php_vm`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just verify-runtime`

End report:
- Explain where the VM implementation now lives.
- List moved files.
- Confirm no new behavior was intentionally added.
```

## Cleanup Prompt B — Extract object metadata from object storage

```text
You are working on current `main` after Cleanup Prompt A.

Goal:
Split `crates/php_runtime/src/object.rs` into clear runtime object infrastructure modules without behavior changes.

Problem:
`object.rs` currently mixes RuntimeType, class metadata, member flags, property hooks, enum metadata, attribute metadata, ObjectStorage, ObjectRef, WeakObjectHandle, and large broad tests.

Tasks:
1. Create:
   - `crates/php_runtime/src/object/mod.rs`
   - `crates/php_runtime/src/object/types.rs`
   - `crates/php_runtime/src/object/class.rs`
   - `crates/php_runtime/src/object/member.rs`
   - `crates/php_runtime/src/object/attribute.rs`
   - `crates/php_runtime/src/object/storage.rs`
   - `crates/php_runtime/src/object/debug.rs`
2. Move without behavior changes:
   - `RuntimeType` -> `types.rs`
   - `ClassEntry`, `ClassFlags` -> `class.rs`
   - method/property/constant/enum entries and flags -> `member.rs`
   - `AttributeEntry` -> `attribute.rs`
   - `ObjectStorage`, `ObjectRef`, `WeakObjectHandle` -> `storage.rs`
   - property debug label helpers -> `debug.rs`
3. Keep public re-exports from `php_runtime::object` and `php_runtime::lib` stable.
4. Move tests into focused modules:
   - identity/storage tests
   - class metadata tests
   - enum metadata tests
   - attribute/reflection metadata tests
   - magic metadata tests
5. No new object behavior.

Acceptance:
- `nix develop -c cargo test -p php_runtime object`
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c just verify-runtime`

End report:
- Mapping old section -> new file.
- Confirm public API compatibility.
```

## Cleanup Prompt C — Make builtin module split real, not registry-only

```text
You are working on current `main` after Cleanup Prompt B.

Goal:
Move builtin implementations into their owning module files. Do not add new builtin behavior.

Problem:
`builtins/modules/filesystem.rs` and `math.rs` are mostly registry slices that call `core::builtin_*`. `core.rs` still owns too much implementation.

Tasks:
1. Keep `builtins/modules/core.rs` limited to:
   - scalar/type helpers
   - shared arity/type/conversion helpers
   - shared debug/output helpers
   - placeholders that are truly cross-module
2. Move implementations:
   - math functions from `core.rs` to `modules/math.rs`
   - filesystem functions from `core.rs` to `modules/filesystem.rs`
   - stream functions from `core.rs` to `modules/streams.rs`
   - JSON functions to `modules/json.rs`
   - PCRE functions to `modules/pcre.rs`
   - date functions to `modules/date.rs`
   - reflection/symbol helpers to `modules/reflection.rs`
3. If helpers are used by many modules, move them to:
   - `builtins/modules/support/args.rs`
   - `builtins/modules/support/arrays.rs`
   - `builtins/modules/support/format.rs`
   - `builtins/modules/support/fs.rs`
   - `builtins/modules/support/debug_dump.rs`
4. Keep `ENTRIES` behavior identical.
5. Do not add or remove registered builtins.
6. Update `docs/runtime-builtin-modules.md`.

Acceptance:
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c just diff-stdlib`
- `nix develop -c just verify-stdlib`

End report:
- Per module: number of builtin implementations moved.
- Any helpers intentionally left in `core.rs`.
```

## Cleanup Prompt D — Separate callable values from generic Value

```text
You are working on current `main` after Cleanup Prompt C.

Goal:
Extract closure/callable-specific runtime value code out of `value.rs` without changing behavior.

Tasks:
1. Create:
   - `crates/php_runtime/src/callable.rs`
2. Move from `value.rs`:
   - `ClosureDebugInfo`
   - `ClosureContext`
   - `ClosurePayload`
   - `CallableValue`
   - `CallableMethodTarget`
   - `ClosureCaptureValue`
   - callable constructors/helpers currently on `Value` where appropriate
3. Keep `Value::Callable(CallableValue)` unchanged.
4. Keep public exports from `php_runtime::lib` stable.
5. Update uses across `php_ir`, `php_vm`, `php_runtime`, and tests.
6. No new callable behavior.

Acceptance:
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just verify-runtime`

End report:
- What moved from `value.rs`.
- What remains in `value.rs` and why.
```

## Cleanup Prompt E — Finish PHPT tools split

```text
You are working on current `main` after Cleanup Prompt D.

Goal:
Finish splitting `crates/php_phpt_tools/src/main.rs` into command modules so future PHPT prompts can orient quickly.

Tasks:
1. Keep `main.rs` as only:
   - imports
   - `main()`
   - command dispatch through `cli`
2. Move code into:
   - `commands/index.rs`
   - `commands/source_index.rs`
   - `commands/symbol_index.rs`
   - `commands/lookup.rs`
   - `commands/triage.rs`
   - `commands/generate.rs`
   - `commands/run.rs`
   - `commands/baseline.rs`
   - `commands/verify.rs`
   - `options/*.rs` if useful
   - `model/*.rs` for JSONL structs
3. Keep existing CLI flags and output stable.
4. Add lightweight unit tests for command option parsing.
5. No PHPT behavior changes.

Acceptance:
- `nix develop -c cargo test -p php_phpt_tools`
- `nix develop -c just verify-phpt`
- `nix develop -c just phpt-triage`

End report:
- New command module map.
- Confirm no output format changed.
```

## Cleanup Prompt F — Make arginfo usable before function work

```text
You are working on current `main` after Cleanup Prompt E.

Goal:
Make generated arginfo real enough for Prompt 13 work.

Problem:
`php_std::arginfo` imports generated arginfo, but `crates/php_std/src/generated/arginfo.rs` is empty.

Tasks:
1. Inspect `scripts/stdlib/generate_arginfo.py`, `just generate-arginfo`, php-src stubs, and overrides.
2. Generate a non-empty `crates/php_std/src/generated/arginfo.rs` or replace it with an include/generated artifact path that is committed.
3. Minimum metadata:
   - function name
   - extension/module
   - parameter names
   - required/optional
   - variadic
   - by-ref
   - simple type atoms
   - nullable
   - simple defaults where available
4. Add a test that fails if generated arginfo is empty.
5. Wire builtin arity/type helpers to consume arginfo where possible.
6. Do not implement new builtins in this prompt.

Acceptance:
- `nix develop -c just generate-arginfo`
- `nix develop -c cargo test -p php_std`
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c just verify-stdlib`

End report:
- Function count, class count, method count if available.
- Unsupported signature features.
```

---

# Rewritten Prompt 13 — Functions, Callables, Arity, Type Coercion

## Prompt 13.1 — Establish focused `zend.functions` harness

```text
You are working on current `main` after Cleanup Prompts A-F.

Goal:
Make `zend.functions` orientation cheap and reproducible before adding behavior.

Tasks:
1. Re-read:
   - `docs/phpt/modules/zend.functions.md`
   - `docs/phpt/prompt-13-handover.md`
   - `tests/phpt/manifests/modules/zend.functions.json`
   - `tests/phpt/manifests/modules/zend.functions.selected.jsonl`
2. Run:
   - `nix develop -c just phpt-dev-build`
   - `nix develop -c just phpt-dev-module MODULE=zend.functions`
   - `nix develop -c just phpt-rerun-failures MODULE=zend.functions`
3. Generate a focused blocker report:
   - `docs/phpt/reports/zend.functions-current.md`
   containing top blocker IDs, counts, top files, and suggested owner layer.
4. Do not implement behavior.

Acceptance:
- `nix develop -c cargo test -p php_phpt_tools`
- `nix develop -c just verify-phpt`

End report:
- Current focused PASS/SKIP/FAIL/BORK.
- Top 10 blocker IDs.
```

## Prompt 13.2 — Call-site line plumbing for frames

```text
You are working on current `main` after Prompt 13.1.

Goal:
Add call-site source line plumbing to VM frames so later ArgumentCountError, TypeError, and backtraces can render PHP-like locations.

Tasks:
1. Add call-site span/line metadata to `crates/php_vm/src/frame.rs`.
2. Thread it through all user-function, method, static method, closure, callable, pipe, and include/eval call paths.
3. Update backtrace rendering to prefer the caller call-site line.
4. Do not change error classes or messages yet.
5. Add VM unit tests using a small function call stack.

Acceptance:
- `nix develop -c cargo test -p php_vm frame`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just verify-runtime`

End report:
- List all call paths updated.
```

## Prompt 13.3 — Builtin arity through arginfo

```text
You are working on current `main` after Prompt 13.2.

Goal:
Route builtin arity validation through generated arginfo for functions used by `zend.functions`.

Scope:
- too few args
- too many args
- optional args
- variadics
- by-ref metadata presence
- no behavior changes to actual function bodies unless required by arity

Tasks:
1. Identify failing `zend.functions` PHPTs whose primary blocker is builtin arity.
2. Use `php_std::arginfo` metadata for arity validation.
3. Keep module-owned builtin implementations in their module files.
4. Add PHPT/generated tests:
   - builtin-too-few-args
   - builtin-too-many-args
   - variadic builtin arity
5. Update `docs/phpt/reports/zend.functions-current.md`.

Acceptance:
- `nix develop -c cargo test -p php_std`
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c just phpt-dev-module MODULE=zend.functions`
- `nix develop -c just verify-stdlib`

End report:
- Builtins now covered by arginfo arity.
- Remaining arity failures.
```

## Prompt 13.4 — User-function argument semantics

```text
You are working on current `main` after Prompt 13.3.

Goal:
Stabilize user-function argument passing.

Scope:
- missing required arguments
- extra positional arguments
- `func_get_args`
- defaults
- variadics
- named args only if already represented cleanly
- by-value argument passing

Tasks:
1. Inspect existing function-call preparation logic.
2. Make user-function extra args visible to `func_get_args`.
3. Make defaults and variadics match PHP 8.5.7 for focused PHPTs.
4. Route missing required args to PHP-like `ArgumentCountError` behavior using call-site plumbing.
5. Add focused generated PHPTs with provenance.

Acceptance:
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.functions`
- `nix develop -c just verify-runtime`

End report:
- Which function argument cases now pass.
- Remaining user-function arg gaps.
```

## Prompt 13.5 — By-reference parameter send MVP

```text
You are working on current `main` after Prompt 13.4.

Goal:
Stabilize by-reference parameter sends for focused `zend.functions` tests.

Scope:
- local variable by-ref sends
- array element by-ref sends if IR already represents them
- property by-ref sends only if existing infrastructure supports it
- by-ref mismatch error rendering

Tasks:
1. Use existing `IrCallArg` by-ref metadata.
2. Fix VM parameter binding for supported lvalue forms.
3. Unsupported lvalue forms must produce deterministic known-gap diagnostics, not silent incorrect behavior.
4. Add generated PHPT:
   - by-ref local ok
   - by-ref mismatch
   - by-ref array element if supported

Acceptance:
- `nix develop -c cargo test -p php_ir`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.functions`

End report:
- Supported by-ref send forms.
- Explicit unsupported forms and IDs.
```

## Prompt 13.6 — Closure internal class MVP

```text
You are working on current `main` after Prompt 13.5.

Goal:
Implement the minimal internal `Closure` runtime class needed by `zend.functions`.

Scope:
- `instanceof Closure`
- first-class callable values expose a Closure object identity
- `var_dump($closure)` basic shape
- `Closure::fromCallable`
- `Closure::bind`, `bindTo`, `call` only as explicit known gaps unless already straightforward

Tasks:
1. Introduce internal class metadata for `Closure`.
2. Avoid fake userland class definitions.
3. Make closure values class-compatible without breaking callable invocation.
4. Add generated PHPTs from `Zend/tests/closures` and `Zend/tests/first_class_callable`.
5. Update Reflection gaps but do not implement Reflection here.

Acceptance:
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.functions`

End report:
- Closure behaviors now passing.
- Closure methods still gaps.
```

## Prompt 13.7 — First-class callable and callable array validation

```text
You are working on current `main` after Prompt 13.6.

Goal:
Stabilize callable acquisition and invocation forms used by `zend.functions`.

Scope:
- plain function first-class callable
- instance method first-class callable
- static method first-class callable
- callable arrays `[$obj, "method"]`, `[ClassName::class, "method"]`
- `is_callable` for covered forms
- invalid callable array diagnostics

Tasks:
1. Reuse `CallableValue` and generated arginfo; do not add source-string matching.
2. Validate callable arrays in VM/runtime helper layer.
3. Make invalid callable arrays PHP-like where possible; otherwise stable gap.
4. Add generated PHPTs:
   - callable invocation
   - is_callable forms
   - invalid callable array

Acceptance:
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.functions`

End report:
- Callable forms supported.
- Remaining callable gaps.
```

## Prompt 13.8 — Weak/strict scalar parameter coercion

```text
You are working on current `main` after Prompt 13.7.

Goal:
Stabilize scalar parameter type coercion for focused function tests.

Scope:
- weak mode scalar coercion
- `declare(strict_types=1)` rejection
- builtin scalar coercion through arginfo
- user-function scalar param checks
- return type checks only if already needed by selected failures

Tasks:
1. Use existing `RuntimeContext` strict-types metadata.
2. Use generated arginfo for builtins.
3. Route mismatches to `TypeError` with PHP-like output.
4. Add generated PHPT:
   - weak coercion
   - strict-types rejection
   - builtin strict/weak behavior where applicable

Acceptance:
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.functions`

End report:
- Coercion matrix covered.
- Remaining type gaps.
```

## Prompt 13.9 — Pipe callable RHS cleanup

```text
You are working on current `main` after Prompt 13.8.

Goal:
Fix remaining `E_PHP_VM_PIPE_RHS_NOT_CALLABLE` failures in `zend.functions`.

Scope:
- pipe RHS closure
- pipe RHS first-class callable
- pipe RHS method callable if supported
- invalid RHS error

Tasks:
1. Inspect `lower_pipe_to_register` and VM pipe execution.
2. Route RHS through unified callable acquisition.
3. Preserve LHS/RHS evaluation order.
4. Add PHPTs:
   - valid pipe callable
   - invalid pipe RHS

Acceptance:
- `nix develop -c cargo test -p php_ir`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.functions`

End report:
- Pipe callable forms supported.
```

## Prompt 13.10 — Close Prompt 13 with focused full-gate

```text
You are working on current `main` after Prompt 13.9.

Goal:
Close the refactored Prompt 13 sequence.

Tasks:
1. Run:
   - `nix develop -c just verify-runtime`
   - `nix develop -c just verify-stdlib`
   - `nix develop -c just phpt-dev-module MODULE=zend.functions`
2. If local full run is available:
   - `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php PHPT_RUN_FULL=1 nix develop -c just phpt-full-regression`
3. Update:
   - `docs/phpt/modules/zend.functions.md`
   - `docs/phpt/reports/zend.functions-current.md`
   - module manifest counts
   - known gaps
4. Do not accept new full-baseline fingerprints unless explicitly justified.

Acceptance:
- Focused `zend.functions` improves.
- No regressions in already-green modules.

End report:
- Before/after counts.
- Remaining blockers by ID.
- Recommendation for next prompt.
```

---

# Rewritten Prompt 14 — Objects, Classes, Magic, Traits, Enums

## Prompt 14.1 — Establish `zend.objects` harness

```text
You are working on current `main` after Prompt 13.10.

Goal:
Create a focused object-module harness before object behavior work.

Tasks:
1. Create or refresh:
   - `docs/phpt/modules/zend.objects.md`
   - `tests/phpt/manifests/modules/zend.objects.json`
   - `tests/phpt/manifests/modules/zend.objects.selected.jsonl`
2. Select a small vertical batch:
   - construction
   - property read/write
   - method call
   - visibility
   - static access
3. Run:
   - `nix develop -c just phpt-generate-module MODULE=zend.objects`
   - `nix develop -c just phpt-dev-module MODULE=zend.objects`
4. Produce blocker report:
   - `docs/phpt/reports/zend.objects-current.md`
5. No behavior changes.

Acceptance:
- `nix develop -c just verify-phpt`
- `nix develop -c just phpt-dev-module MODULE=zend.objects`

End report:
- Focused object failures grouped by construction/property/method/visibility/static.
```

## Prompt 14.2 — Class table and internal class lookup hygiene

```text
You are working after Prompt 14.1.

Goal:
Stabilize class lookup before adding behavior.

Tasks:
1. Review IR `ClassEntry` and runtime `ClassEntry` conversion path.
2. Centralize class-name normalization and display-name preservation.
3. Ensure internal classes introduced by runtime, such as `Closure` and throwable classes, share the same lookup path as user classes.
4. No magic methods, traits, enums, or property hooks yet.

Acceptance:
- `nix develop -c cargo test -p php_ir`
- `nix develop -c cargo test -p php_runtime object`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.objects`

End report:
- Class lookup rules and display-name rules.
```

## Prompt 14.3 — Constructor, property, method basics

```text
You are working after Prompt 14.2.

Goal:
Make basic class construction, property read/write, and instance method calls pass.

Scope:
- `new C`
- `__construct`
- public properties
- public methods
- `$this`
- basic method return

Tasks:
1. Fix only basic object behavior.
2. Add generated PHPTs:
   - constructor-property
   - property-read-write
   - method-call
   - `$this` inside method
3. Update object module report.

Acceptance:
- `nix develop -c cargo test -p php_runtime object`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.objects`

End report:
- Basic object cases passing.
```

## Prompt 14.4 — Visibility and PHP Error routing

```text
You are working after Prompt 14.3.

Goal:
Implement PHP-like errors for private/protected property and method access.

Scope:
- private property read/write
- protected property read/write
- private method external call
- protected method external call
- catchable `Error` routing
- exact-ish PHP messages

Tasks:
1. Reuse the VM runtime error throwable routing infrastructure.
2. Do not silently return null/false for visibility violations.
3. Add PHPTs:
   - private property external error
   - protected method external error
   - catch(Error) catches visibility error

Acceptance:
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.objects`

End report:
- Visibility cases fixed.
- Remaining message wording gaps.
```

## Prompt 14.5 — Static properties and static methods

```text
You are working after Prompt 14.4.

Goal:
Stabilize static object access.

Scope:
- public static methods
- static properties
- `self::`
- `static::`
- `parent::` only where metadata is available
- invalid static scope as catchable Error where appropriate

Tasks:
1. Verify reference behavior for each selected PHPT.
2. Fix valid cases before converting invalid cases to errors.
3. Add PHPTs for:
   - public static method
   - static property read/write
   - invalid static scope
4. Do not implement late static binding beyond the focused cases.

Acceptance:
- `nix develop -c cargo test -p php_ir`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.objects`

End report:
- Static cases passing.
- Explicit LSB gaps.
```

## Prompt 14.6 — Typed and uninitialized properties

```text
You are working after Prompt 14.5.

Goal:
Fix typed property initialization and uninitialized property errors.

Scope:
- uninitialized typed property read
- property type mismatch
- nullable property
- default values
- readonly only if already represented cleanly

Tasks:
1. Ensure object storage can distinguish uninitialized from null.
2. Route uninitialized property access to PHP-like `Error`.
3. Enforce focused property type writes.
4. Add PHPTs:
   - typed property uninitialized
   - nullable property
   - property type mismatch

Acceptance:
- `nix develop -c cargo test -p php_runtime object`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.objects`

End report:
- Typed property coverage.
```

## Prompt 14.7 — Magic property and method MVP

```text
You are working after Prompt 14.6.

Goal:
Implement focused magic method behavior.

Scope:
- `__get`
- `__set`
- `__isset`
- `__unset`
- `__call`
- `__callStatic`
- `__invoke`
- `__toString` only for focused string contexts

Tasks:
1. Implement only PHPT-driven magic behavior.
2. Guard recursive magic calls with deterministic diagnostics.
3. Add focused PHPTs for each supported magic method.
4. Do not implement serialization magic here.

Acceptance:
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.objects`

End report:
- Magic methods supported.
- Magic gaps.
```

## Prompt 14.8 — Clone and clone-with MVP

```text
You are working after Prompt 14.7.

Goal:
Stabilize clone and PHP 8.5 clone-with for focused object tests.

Scope:
- shallow clone
- `__clone`
- public property replacements
- typed public replacement checks
- clone-with set hook only if already supported

Tasks:
1. Verify reference output for selected PHPTs.
2. Implement public property clone-with only.
3. Private/protected/readonly replacement remains documented gap unless already straightforward.
4. Add PHPTs:
   - clone identity
   - clone independent properties
   - clone-with public property
   - clone-with unsupported private/readonly gap

Acceptance:
- `nix develop -c cargo test -p php_runtime object`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.objects`

End report:
- Clone/clone-with coverage.
```

## Prompt 14.9 — Traits and enums MVP

```text
You are working after Prompt 14.8.

Goal:
Implement focused traits and enums only after object basics are stable.

Scope:
- trait method composition MVP
- trait method alias/precedence only if selected PHPTs require it
- unit enum cases
- backed enum cases
- `cases`
- `from`
- `tryFrom`
- enum methods

Tasks:
1. Split trait work and enum work internally, but keep this prompt focused on selected PHPTs.
2. Add generated PHPTs with provenance.
3. Document all wider trait/enum gaps.

Acceptance:
- `nix develop -c cargo test -p php_runtime object`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.objects`

End report:
- Traits covered.
- Enums covered.
- Gaps.
```

## Prompt 14.10 — Close Prompt 14

```text
You are working after Prompt 14.9.

Goal:
Close object work and update tracking.

Tasks:
1. Run:
   - `nix develop -c just verify-runtime`
   - `nix develop -c just phpt-dev-module MODULE=zend.objects`
2. Run full regression if available.
3. Update:
   - `docs/phpt/modules/zend.objects.md`
   - `docs/phpt/reports/zend.objects-current.md`
   - runtime known gaps
   - module manifest counts
4. Do not start filesystem/stdlib work.

Acceptance:
- Object module improved.
- No new known full-regression fingerprints unless justified.

End report:
- Before/after object counts.
- Next blockers.
```

---

# Rewritten Prompt 15 — Filesystem, Streams, Resources, Include

## Prompt 15.1 — Realign filesystem/stream builtin ownership

```text
You are working after Prompt 14.10.

Goal:
Before implementing filesystem behavior, ensure filesystem and stream functions live in their owning modules, not in `core.rs`.

Tasks:
1. Verify Cleanup Prompt C was completed.
2. If not, move filesystem implementations to `builtins/modules/filesystem.rs` and stream implementations to `streams.rs`.
3. Keep behavior unchanged.
4. Update docs if needed.

Acceptance:
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c just diff-streams`
- `nix develop -c just verify-stdlib`
```

## Prompt 15.2 — Establish `filesystem.streams` harness

```text
Goal:
Create a focused PHPT harness for filesystem and streams.

Tasks:
1. Create/update:
   - `docs/phpt/modules/filesystem.streams.md`
   - `tests/phpt/manifests/modules/filesystem.streams.json`
   - `tests/phpt/manifests/modules/filesystem.streams.selected.jsonl`
2. Select focused PHPTs:
   - cwd
   - `file_exists`
   - `file_get_contents`
   - `file_put_contents`
   - `fopen/fread/fwrite/fclose`
   - include/require local files
3. Run:
   - `nix develop -c just phpt-dev-module MODULE=filesystem.streams`
4. Produce blocker report.

Acceptance:
- `nix develop -c just verify-phpt`
```

## Prompt 15.3 — Request-local CWD, include_path, resource table persistence

```text
Goal:
Make BuiltinContext state persistent across VM builtin calls.

Scope:
- cwd
- include_path
- resource table
- stream handles
- JSON/PCRE last-error state only if needed by shared context design

Tasks:
1. Move state ownership out of per-call temporary BuiltinContext if it is recreated each dispatch.
2. Ensure builtins see and mutate request-local state.
3. Add VM tests:
   - chdir then getcwd
   - fopen then fwrite/fread/fclose
   - include_path set/read if supported

Acceptance:
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=filesystem.streams`
```

## Prompt 15.4 — Local filesystem functions

```text
Goal:
Stabilize local filesystem builtins under deterministic root constraints.

Scope:
- `file_exists`
- `is_file`
- `is_dir`
- `filesize`
- `filemtime`
- `file_get_contents`
- `file_put_contents`
- `readfile`
- `unlink`
- `rename`
- `mkdir`
- `rmdir`
- warnings for invalid/missing files

Tasks:
1. Use deterministic filesystem capabilities.
2. Keep network URLs out of scope.
3. Add PHPTs for local temp files and missing file warnings.
4. Update known gaps for warning byte parity.

Acceptance:
- `nix develop -c just diff-streams`
- `nix develop -c just phpt-dev-module MODULE=filesystem.streams`
```

## Prompt 15.5 — Streams and resources MVP

```text
Goal:
Stabilize stream resources.

Scope:
- `fopen`
- `fclose`
- `fread`
- `fwrite`
- `feof`
- `ftell`
- `fseek`
- `rewind`
- `stream_get_contents`
- `stream_get_meta_data`
- `php://memory`
- `php://temp`

Tasks:
1. Keep resource identity stable.
2. Implement only local/php wrapper streams.
3. Add PHPTs for read/write/seek/meta.
4. No network streams and no stream filters.

Acceptance:
- `nix develop -c cargo test -p php_runtime resource`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=filesystem.streams`
```

## Prompt 15.6 — Include/require local semantics

```text
Goal:
Stabilize local include/require behavior.

Scope:
- include return value
- require fatal on missing
- include warning on missing
- include scope variable sharing
- include_once/require_once
- include_path search

Tasks:
1. Use existing single frontend/IR/VM pipeline.
2. Do not implement remote wrappers or PHAR.
3. Add PHPTs:
   - include return
   - include shares variable
   - include_once
   - require missing fatal
   - include_path local search

Acceptance:
- `nix develop -c cargo test -p php_vm include`
- `nix develop -c just phpt-dev-module MODULE=filesystem.streams`
```

## Prompt 15.7 — Close Prompt 15

```text
Goal:
Close filesystem/streams and update reports.

Tasks:
1. Run:
   - `nix develop -c just verify-runtime`
   - `nix develop -c just verify-stdlib`
   - `nix develop -c just phpt-dev-module MODULE=filesystem.streams`
2. Run full regression if available.
3. Update module report and known gaps.

End report:
- Filesystem functions passing.
- Streams passing.
- Include gaps.
```

---

# Rewritten Prompt 16 — ext/standard core modules

## Prompt 16.1 — Create standard-core dashboard

```text
You are working after Prompt 15.7.

Goal:
Create a standard-library core dashboard before implementing functions.

Tasks:
1. Create:
   - `docs/phpt/reports/standard-core-dashboard.md`
2. Include modules:
   - standard.arrays
   - standard.strings
   - standard.math
   - standard.variables
   - standard.output
   - standard.serialization
   - standard.url-html
3. For each module, record PASS/SKIP/FAIL/BORK, top blocker IDs, owning files.
4. No behavior changes.

Acceptance:
- `nix develop -c just phpt-triage`
- `nix develop -c just verify-phpt`
```

## Prompt 16.2 — standard.math

```text
Goal:
Make `standard.math` focused PHPTs pass.

Scope:
- math functions only
- generated arginfo arity/type
- numeric conversion via existing conversion layer

Tasks:
1. Ensure math implementations live in `builtins/modules/math.rs`.
2. Run `nix develop -c just phpt-dev-module MODULE=standard.math`.
3. Fix only math functions.
4. Add PHPTs for edge cases.

Acceptance:
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c just phpt-dev-module MODULE=standard.math`
- `nix develop -c just verify-stdlib`
```

## Prompt 16.3 — standard.variables

```text
Goal:
Make variable/type helper PHPTs pass.

Scope:
- `gettype`
- `is_*`
- `empty`
- `isset`
- `var_dump`
- `print_r`
- `var_export`
- scalar/object/array formatting gaps needed by selected tests

Tasks:
1. Keep debug formatting helpers in a support module.
2. Fix output shape by PHPT oracle.
3. Do not implement Reflection here.

Acceptance:
- `nix develop -c just phpt-dev-module MODULE=standard.variables`
- `nix develop -c just verify-stdlib`
```

## Prompt 16.4 — standard.arrays builtins

```text
Goal:
Make standard array builtins pass after array runtime is stable.

Scope:
- `count`
- `array_keys`
- `array_values`
- `array_merge`
- `array_slice`
- `array_map` only if callable dispatch is ready
- sort functions only if deterministic comparator exists

Tasks:
1. Use `builtins/modules/arrays.rs`.
2. Avoid VM callback shortcuts unless routed through unified callable path.
3. Add PHPTs for each fixed function.

Acceptance:
- `nix develop -c just phpt-dev-module MODULE=standard.arrays`
- `nix develop -c just verify-stdlib`
```

## Prompt 16.5 — standard.strings builtins

```text
Goal:
Make standard string builtins pass.

Scope:
- `strlen`
- `substr`
- `strpos`
- `str_contains`
- `trim`
- `explode`
- `implode`
- `sprintf/printf`
- `strtok` state
- binary-safe behavior

Tasks:
1. Use `builtins/modules/strings.rs`.
2. Keep request-local `strtok` state in BuiltinContext/VM state.
3. Use PHPT oracle for output and warnings.

Acceptance:
- `nix develop -c just phpt-dev-module MODULE=standard.strings`
- `nix develop -c just verify-stdlib`
```

## Prompt 16.6 — standard.output

```text
Goal:
Make output-buffer-related standard PHPTs pass where supported.

Scope:
- `ob_start`
- `ob_get_contents`
- `ob_get_clean`
- `ob_get_length`
- `ob_get_level`
- `ob_end_clean`
- `ob_end_flush`
- nested output buffers

Tasks:
1. Implement VM-level output buffer stack, not fake builtin-local buffers.
2. Keep callback output handlers as explicit gaps unless callable dispatch supports them.
3. Add PHPTs for nested buffers.

Acceptance:
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=standard.output`
```

## Prompt 16.7 — standard.serialization

```text
Goal:
Make serialization core PHPTs pass.

Scope:
- `serialize`
- `unserialize`
- scalar values
- arrays
- simple objects
- recursion guards
- references only if existing reference model supports them

Tasks:
1. Use existing serialization module.
2. Do not fake unsupported reference records.
3. Add PHPTs for scalar/array/object basics.
4. Document `R`/`r` reference gaps.

Acceptance:
- `nix develop -c cargo test -p php_runtime serialization`
- `nix develop -c just phpt-dev-module MODULE=standard.serialization`
```

## Prompt 16.8 — standard.url-html

```text
Goal:
Make common URL/HTML helper PHPTs pass.

Scope:
- `urlencode`
- `urldecode`
- `rawurlencode`
- `rawurldecode`
- `http_build_query` MVP
- `htmlspecialchars`
- `htmlentities` MVP

Tasks:
1. Keep full charset/entity tables documented as gaps.
2. Implement common flags only when PHPT-driven.
3. Add generated PHPTs.

Acceptance:
- `nix develop -c just phpt-dev-module MODULE=standard.url-html`
- `nix develop -c just verify-stdlib`
```

## Prompt 16.9 — Close Prompt 16

```text
Goal:
Close ext/standard core module batch.

Tasks:
1. Run all standard core module gates.
2. Run `nix develop -c just verify-stdlib`.
3. Run full regression if available.
4. Update dashboard and module reports.

End report:
- Table: module, before fail/bork, after fail/bork, remaining gaps.
```

---

# Rewritten Prompt 17 — JSON

## Prompt 17.1 — Establish JSON harness

```text
You are working after Prompt 16.9.

Goal:
Create focused JSON module harness.

Tasks:
1. Create/update:
   - `docs/phpt/modules/json.md`
   - `tests/phpt/manifests/modules/json.json`
   - `tests/phpt/manifests/modules/json.selected.jsonl`
2. Select focused PHPTs:
   - json_encode scalar/array/object basics
   - json_decode object/array basics
   - json_last_error
   - json_last_error_msg
   - common flags
3. Run `nix develop -c just phpt-dev-module MODULE=json`.
4. Produce `docs/phpt/reports/json-current.md`.

Acceptance:
- `nix develop -c just verify-phpt`
```

## Prompt 17.2 — JSON state persistence

```text
Goal:
Fix JSON request-local last-error state.

Scope:
- `json_last_error`
- `json_last_error_msg`
- successful decode clears state
- invalid decode sets state
- state persists across builtin calls in same request

Tasks:
1. Ensure BuiltinContext/VM request state persists JSON last-error.
2. Add VM/runtime tests.
3. Add PHPTs:
   - invalid decode then last_error
   - valid decode then no error

Acceptance:
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=json`
```

## Prompt 17.3 — json_encode MVP

```text
Goal:
Stabilize `json_encode`.

Scope:
- null/bool/int/float/string
- arrays as lists/objects
- simple objects
- common flags:
  - JSON_PRETTY_PRINT
  - JSON_UNESCAPED_SLASHES
  - JSON_UNESCAPED_UNICODE
  - JSON_PRESERVE_ZERO_FRACTION
  - JSON_THROW_ON_ERROR where error path already supports exceptions

Tasks:
1. Use php-src/reference output for selected flags.
2. Keep bigint/UTF-8 edge cases documented as gaps unless selected PHPT requires them.
3. Add generated PHPTs.

Acceptance:
- `nix develop -c just phpt-dev-module MODULE=json`
- `nix develop -c just diff-json-pcre-date`
```

## Prompt 17.4 — json_decode MVP

```text
Goal:
Stabilize `json_decode`.

Scope:
- scalar JSON
- arrays
- objects as associative arrays via flag
- depth errors
- syntax errors
- common flags
- `JSON_THROW_ON_ERROR`

Tasks:
1. Use request-local error state from Prompt 17.2.
2. Add PHPTs for success and failure.
3. Keep advanced UTF-8/bigint behavior as explicit gaps unless selected PHPTs require it.

Acceptance:
- `nix develop -c just phpt-dev-module MODULE=json`
- `nix develop -c just diff-json-pcre-date`
```

## Prompt 17.5 — JsonSerializable integration

```text
Goal:
Implement JsonSerializable only if object/method dispatch from Prompt 14 supports it cleanly.

Scope:
- class implements JsonSerializable
- call `jsonSerialize`
- encode returned scalar/array/object

Tasks:
1. If interface/method dispatch is not ready, document as known gap and do not fake it.
2. If ready, route through normal VM method call path.
3. Add PHPTs.

Acceptance:
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=json`
```

## Prompt 17.6 — Close Prompt 17

```text
Goal:
Close JSON module.

Tasks:
1. Run:
   - `nix develop -c just diff-json-pcre-date`
   - `nix develop -c just phpt-dev-module MODULE=json`
   - `nix develop -c just verify-stdlib`
2. Run full regression if available.
3. Update:
   - `docs/phpt/modules/json.md`
   - `docs/phpt/reports/json-current.md`
   - JSON known gaps

End report:
- JSON PASS/SKIP/FAIL/BORK before/after.
- Remaining JSON gaps.
```

---

## Recommended immediate execution order

Run these first, before any more Prompt 13 feature work:

1. **Cleanup A** — restore VM module integrity.
2. **Cleanup B** — split object metadata/storage.
3. **Cleanup C** — make builtin module split real.
4. **Cleanup D** — extract callable runtime values.
5. **Cleanup E** — finish PHPT tools split.
6. **Cleanup F** — make generated arginfo non-empty and tested.

Then continue with **13.1 → 13.10**. That sequence should stop the “spend all time orienting” pattern because each prompt has one owner layer, one focused behavior cluster, and a small acceptance gate.
