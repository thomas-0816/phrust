# Runtime Semantics Status

For the final Runtime semantics closure state, use `docs/runtime/semantics-validation.md`,
`docs/runtime/semantics-coverage-matrix.md`, `docs/runtime/gap-closure-plan.md`,
and `docs/stdlib/roadmap.md`.

This document records the current runtime semantics position for the PHP engine
work. It is a status and deferred-scope document, not a compatibility claim.

## Runtime semantics Position

Runtime semantics moves the executable core from the initial runtime subset toward PHP runtime
semantics: references, Copy-on-Write, arrays, foreach, calls, objects, traits,
interfaces, enums, attributes, Reflection metadata, generators, fibers,
include/eval/autoload/globals, diagnostics, destructors, GC debug hooks,
runtime tracing, regression minimization, and real-world smoke fixtures.

The source pipeline remains single-path:

```text
php_lexer -> php_syntax -> php_ast -> php_semantics -> php_ir -> php_runtime -> php_vm
```

No Runtime semantics feature should be considered complete unless it has a fixture,
known-gap entry where appropriate, and validation through the relevant
`just` gate. `just runtime-gap-report` regenerates the executable compatibility
gap closure report under `target/runtime-gap-report/` and refreshes the
committed closure plan. Current topic docs are:

| Area | Document |
| --- | --- |
| Runtime contract and public APIs | `docs/runtime/semantics-contract.md` |
| References and COW | `docs/runtime/semantics-reference-cow.md` |
| Arrays and foreach | `docs/runtime/semantics-array-semantics.md`, `docs/runtime/semantics-foreach-semantics.md` |
| Objects, traits, enums, hooks | `docs/runtime/semantics-object-semantics.md` |
| Generators and fibers | `docs/runtime/semantics-generators-fibers.md` |
| Reflection and attributes | `docs/runtime/semantics-reflection-attributes.md` |
| Specific unsupported behavior | `docs/runtime/semantics-known-gaps.md` |

## Runtime semantics Decision Record

| Decision | ADR |
| --- | --- |
| Destructor queue | `docs/runtime/semantics-contract.md` |
| GC skeleton and root tracking | `docs/runtime/semantics-contract.md` |
| Slot/reference/COW model | `docs/runtime/semantics-reference-cow.md` |
| Array element references and foreach | `docs/runtime/semantics-array-semantics.md` |
| Object model, traits, enums, hooks | `docs/runtime/semantics-object-semantics.md` |
| Generator/fiber control flow | `docs/runtime/semantics-generators-fibers.md` |

## Standard library Public API Starting Points

Standard library should reuse the public Rust surface already documented in
`docs/runtime/semantics-contract.md`:

- `php_vm::api::{Vm, VmOptions, VmResult, CompiledUnit, IncludeLoader}`;
- `php_runtime::api` for `Value`, `Slot`, `ReferenceCell`, `PhpArray`,
  `PhpString`, `ObjectRef`, `CallableValue`, `GeneratorRef`, `FiberRef`, and
  `RuntimeDiagnostic`;
- `php_runtime::api` for `ClassEntry`, `RuntimeType`, `AttributeEntry`,
  `GlobalSymbolTable`, `AutoloadRegistry`, and `BuiltinRegistry`;
- `php_runtime::debug` for GC debug root APIs and other intentionally
  non-PHP-visible inspection helpers.

VM frame internals, continuation structs, GC debug IDs, and trace formatting
are implementation details unless a Standard library ADR stabilizes them.

## Standard Library Follow-Ups

### Standard Library

- Build a compatibility matrix for common framework and Composer helpers:
  `count`, `array_map`, `array_filter`, `array_values`, `array_key_exists`,
  `in_array`, `is_subclass_of`, `class_parents`, `class_implements`,
  string/path helpers, JSON helpers, date/time basics, and error/exception
  helper functions.
- Replace ad hoc builtin gaps with arity/type/error fixtures and reference
  diffs for each supported builtin.
- Keep serialization explicit: implement `serialize`, `unserialize`,
  `var_export`, `__serialize`, `__unserialize`, `__sleep`, `__wakeup`,
  `Serializable`, enum serialization, references, cycles, and allowed-class
  options together rather than one partial string format at a time.

### Streams and Filesystem

- Specify include-path, cwd, realpath cache, stream wrapper, file URL, and
  warning-channel behavior before broadening include/require.
- Add root-constrained test fixtures for local filesystem reads and writes
  before any Composer or framework smoke depends on them.
- Keep remote/network wrappers out of scope unless a later ADR explicitly adds
  them.

### SPL and Iteration

- Expand the current Iterator/IteratorAggregate metadata subset into real SPL
  interface and class surfaces.
- Implement `ArrayAccess` offset reads/writes/isset/unset before claiming
  collection-library compatibility.
- Add fixture matrices for public-property foreach, Iterator,
  IteratorAggregate, ArrayAccess, mutation during iteration, and exception
  ordering.

### Reflection Full Expansion

- Add `ReflectionClass::newInstanceArgs`, constructor invocation, method and
  function invocation APIs, full enum APIs, doc comments, parameter defaults,
  attributes with `newInstance()`, and autoload-sensitive Reflection behavior.
- Ensure Reflection output uses source spelling while runtime lookup continues
  to use normalized names.
- Diff framework-style dependency-injection patterns that rely on Reflection
  before calling the Reflection surface complete.

### Composer and Framework Smokes

- Keep committed smokes offline and handwritten. Do not vendor Composer
  projects into the repo.
- Use `just local-composer-smoke <path>` or a Standard library successor for
  user-provided local projects.
- Prioritize PSR-4 autoloading, class existence probes, Reflection-driven
  containers, enum-backed configuration, attributes, closures, and common
  stdlib helpers before attempting a full framework boot.

### Extensions and ABI

- Zend extension ABI, resources, FFI, FPM/SAPI, Opcache, quickening, inline
  caches, and JIT remain out of scope until a new layer explicitly adds them.
- If resources are introduced for stdlib compatibility, model them as a
  bounded runtime type first and keep extension ABI compatibility as a separate
  decision.

### Performance

- Measure before optimizing. Likely hot paths are array append and COW
  separation, reference-cell writes, foreach snapshots, object method/property
  lookup, Reflection metadata construction, callable resolution,
  include/eval/autoload recompilation, generator/fiber continuation cloning,
  numeric-string classification, and GC root scanning.
- Any cache must be invalidated by include/eval/autoload additions to request
  symbol tables.
- Performance work must keep fixture output, side-effect order, diagnostics,
  and known-gap behavior unchanged.

## Regression and Debug Workflow

Runtime tracing is opt-in with `php-vm run --trace-runtime`. Diff failures
should be minimized with `scripts/minimize_runtime_failure.py` and retained in
`fixtures/runtime_semantics/regressions/pass/` or
`fixtures/runtime_semantics/regressions/known_gaps/` with the required inline metadata.
`just regression-fixtures` is part of `just runtime-semantics-fixtures`; the
regression corpus also runs inside `just verify-runtime` through the
full-corpus differential, which supersets every category gate.

## Compatibility Oracle Requirements

Runnable fixtures under `fixtures/runtime_semantics/**/*.php` require
`REFERENCE_PHP` by default. Missing reference execution is a gate failure unless
the fixture declares `php_ref_optional_reason=<reason>` in its inline metadata.
Known-gap fixtures must carry a stable `known_gap=<ID>` and are counted
separately from pass/fail fixtures.

`just runtime-semantics-diff` compares PHP-visible exit status, stdout, and
normalized stderr against the PHP 8.5.7 reference binary. Phrust's `baseline`
and `default` policies both use the mandatory native compiler; focused preset
parity is covered by `just default-profile-smoke`. `just verify-runtime` runs
the external oracle and native architecture gates.

## Runtime Baseline

The remaining sections record the runtime baseline that the runtime semantics
layer builds on.

## Validation Results

Baseline validation set, run on 2026-06-21:

| Command | Result | Notes |
| --- | --- | --- |
| `nix develop -c just verify-foundation` | pass | Foundation gate preserved |
| `nix develop -c just verify-lexer` | pass | Lexer gate preserved |
| `nix develop -c just verify-frontend` | pass | Parser/CST gate preserved |
| `nix develop -c just verify-frontend` | pass | Semantic frontend gate preserved |
| `nix develop -c just verify-runtime` | pass | IR/VM/runtime gate, including corpus smoke |
| `nix develop -c cargo test --workspace` | pass | Workspace tests pass |

The hard Runtime gate also runs `runtime-fixtures`, `runtime-corpus-smoke`,
`phpt-smoke`, `runtime-known-gaps`, bytecode snapshots, Rust formatting,
Clippy, the Semantic frontend gate, `runtime-semantics-diff`, and the native
product gate. Full runtime verification requires `REFERENCE_PHP` to
point at the PHP 8.5.7 reference binary; use narrower non-reference gates only
when that binary is intentionally unavailable.

## Decision Record

Runtime decisions are captured in these ADRs:

| Decision | ADR |
| --- | --- |
| IR style | `docs/runtime/semantics-status.md` |
| VM dispatch | `docs/runtime/semantics-status.md` |
| Value representation | `docs/runtime/semantics-status.md` |
| Array model | `docs/runtime/semantics-status.md` |
| Object model | `docs/runtime/semantics-status.md` |
| Exception model | `docs/runtime/semantics-status.md` |
| Include model | `docs/runtime/semantics-status.md` |
| Known-gap policy | `docs/adr/0010-runtime-known-gap-policy.md` |

## Feature Matrix

| Feature | Syntax supported | HIR supported | IR supported | VM execution | Reference diff status | Known gap ID |
| --- | --- | --- | --- | --- | --- | --- |
| Scalars and echo | yes | yes | yes | yes | green curated fixtures | none |
| Local variables and assignment | yes | yes | yes | partial | curated assignment and undefined-variable fixtures pass | none |
| Arithmetic, concat, comparisons, casts | yes | yes | yes | partial | numeric-string edge cases differ | `E_PHP_RUNTIME_NUMERIC_STRING_MATRIX` |
| Direct user functions | yes | yes | yes | yes | green curated fixtures | none |
| Defaults, variadics, returns | yes | yes | yes | partial | PHP type/coercion wording differs | `E_PHP_RUNTIME_WEAK_STRICT_TYPES_COERCION` |
| Closures and arrow functions | yes | yes | yes | partial | by-value/by-reference captures, static closure locals, and arrow by-value captures execute; full Closure binding remains deferred | `E_PHP_RUNTIME_UNSUPPORTED_CLOSURE_BINDING` |
| Dynamic function/callable forms | yes | yes | yes | partial | dynamic strings including namespaced strings, array callables, invokable objects, and first-class callables pass curated fixtures | `E_PHP_RUNTIME_UNSUPPORTED_CLOSURE_BINDING` |
| PHP 8.5 pipe operator | yes | yes | yes | partial | callable dispatch and non-callable RHS errors pass curated fixtures | none |
| Selected builtins | yes | yes | yes | partial | strict supported subset only | `E_PHP_RUNTIME_UNSUPPORTED_STDLIB` |
| Arrays | yes | yes | yes | partial | key/COW/reference edges differ | `E_PHP_RUNTIME_ARRAY_REFERENCE_COW` |
| Foreach over arrays and Traversable sources | yes | yes | yes | partial | arrays, public-property objects, Iterator, IteratorAggregate, generator sources, and invalid-source warnings pass curated fixtures | `E_PHP_RUNTIME_FOREACH_MUTATION_COMPAT` |
| References | yes | partial | partial | partial | simple local alias only | `E_PHP_RUNTIME_UNSUPPORTED_REFERENCE_SEMANTICS` |
| Global and magic constants | yes | partial | partial | partial | fixture-covered predefined, user-defined, and scalar cast default constants pass; full constant-expression matrix remains | `E_PHP_RUNTIME_CONST_EXPR_MATRIX` |
| Include/require | yes | yes | yes | partial | root-constrained local model | `E_PHP_RUNTIME_INCLUDE_SCOPE_MATRIX` |
| Concrete classes and `new` | yes | yes | yes | partial | public concrete class subset | `E_PHP_IR_UNSUPPORTED_CLASSLIKE_OBJECT` |
| Public properties and methods | yes | yes | yes | partial | visibility/inheritance missing | `E_PHP_IR_UNSUPPORTED_OBJECT_PROPERTY_MODIFIER` |
| Static methods | yes | partial | partial | partial | explicit class names only | `E_PHP_IR_UNSUPPORTED_LATE_STATIC_BINDING` |
| Clone and clone-with | yes | yes | yes | partial | public shallow subset only | `E_PHP_RUNTIME_UNSUPPORTED_CLONE_WITH_PROPERTY_RULES` |
| Exceptions | yes | yes | yes | partial | internal Exception subset | `E_PHP_RUNTIME_UNSUPPORTED_THROWABLE_HIERARCHY` |
| Runtime type checks | yes | yes | yes | partial | exact family checks only | `E_PHP_RUNTIME_WEAK_STRICT_TYPES_COERCION` |
| Superglobals | yes | partial | partial | partial | controlled CLI subset only | `E_PHP_RUNTIME_SUPERGLOBALS_FULL_MATRIX` |
| Generators and `yield from` | yes | yes | classified | no | known gap | `E_PHP_IR_UNSUPPORTED_GENERATOR` |
| Fibers | yes | yes | classified | no | known gap | `E_PHP_IR_UNSUPPORTED_FIBER` |
| Eval | yes | yes | classified | no | known gap | `E_PHP_IR_UNSUPPORTED_EVAL` |
| Autoload, traits, enums, reflection | yes | yes | partial | partial | autoload, trait, enum, and reflection subsets pass curated fixtures; wider parity remains | `E_PHP_IR_UNSUPPORTED_REFLECTION` |

## Top 20 Reference Deviations by Runtime semantics Risk

1. Full references and Copy-on-Write: local aliases work, but parameters,
   returns, array elements, foreach references, and object-property references
   are gaps. ID: `E_PHP_RUNTIME_UNSUPPORTED_REFERENCE_SEMANTICS`.
2. Array COW and element references: array mutation is not zval-compatible
   across aliases. ID: `E_PHP_RUNTIME_ARRAY_REFERENCE_COW`.
3. Full array semantics: key conversion, spread, packed/hash transitions, and
   invalid-key behavior are incomplete. IDs:
   `E_PHP_RUNTIME_ARRAY_KEY_CONVERSION_EDGE_CASES`,
   `E_PHP_IR_UNSUPPORTED_ARRAY_SPREAD`.
4. Object model depth: inheritance, interfaces, traits, enums, visibility,
   readonly, hooks, dynamic names, and magic methods are not implemented.
   IDs: `E_PHP_IR_UNSUPPORTED_CLASSLIKE_OBJECT`,
   `E_PHP_IR_UNSUPPORTED_OBJECT_PROPERTY_MODIFIER`,
   `E_PHP_RUNTIME_UNSUPPORTED_MAGIC_METHODS`.
5. Autoloading: request-local SPL callbacks execute for covered class,
   interface, static class-like lookup, and handwritten Composer-style fixtures.
   Real Composer metadata fingerprinting and the wider SPL autoload API remain
   incomplete. IDs: `E_PHP_VM_AUTOLOAD_INVALID_CALLBACK`,
   `E_PHP_VM_AUTOLOAD_ARITY`, `E_PHP_RUNTIME_COMPOSER_AUTOLOAD_MATRIX`.
6. Include compatibility: include_path, stream wrappers, cwd policy, and
   complete cross-file symbol side effects are missing. IDs:
   `E_PHP_VM_INCLUDE_MISSING`, `E_PHP_RUNTIME_INCLUDE_SCOPE_MATRIX`.
7. Standard library and extensions: only selected builtins exist. IDs:
   `E_PHP_RUNTIME_UNSUPPORTED_STDLIB`,
   `E_PHP_RUNTIME_BUILTIN_ARITY`,
   `E_PHP_RUNTIME_BUILTIN_TYPE`.
8. Throwable/Error hierarchy: exceptions execute through an internal subset, not
   full PHP `Throwable` classes or stack traces. ID:
   `E_PHP_RUNTIME_UNSUPPORTED_THROWABLE_HIERARCHY`.
9. Type coercion: runtime parameter, return, and property checks do not
   implement PHP weak/strict coercion matrices. ID:
   `E_PHP_RUNTIME_WEAK_STRICT_TYPES_COERCION`.
10. Numeric-string conversion and comparison: only simple cases are covered.
    ID: `E_PHP_RUNTIME_NUMERIC_STRING_MATRIX`.
11. Superglobals and request state: CLI argv/env are controlled; SAPI request
    state and `$GLOBALS` aliasing are not complete. IDs:
    `E_PHP_RUNTIME_SUPERGLOBALS_FULL_MATRIX`,
    `E_PHP_RUNTIME_GLOBALS_ALIAS_MATRIX`.
12. Dynamic calls and callable resolution: dynamic string calls including
    namespaced strings, method callables, array callables, invokable objects,
    and first-class callables are executable for covered fixtures;
    imported/function-alias callable edges, invalid-callable edge cases, and
    closure binding remain incomplete. IDs:
    `E_PHP_VM_INVALID_CALLABLE_ARRAY`,
    `E_PHP_RUNTIME_UNSUPPORTED_CLOSURE_BINDING`.
13. Generators and `yield from`: classified at lowering, not executable.
    IDs: `E_PHP_IR_UNSUPPORTED_GENERATOR`,
    `E_PHP_IR_UNSUPPORTED_YIELD_FROM`.
14. Fibers: no scheduling or suspend/resume model. ID:
    `E_PHP_IR_UNSUPPORTED_FIBER`.
15. Eval: runtime source parsing/execution is not supported. ID:
    `E_PHP_IR_UNSUPPORTED_EVAL`.
16. Reflection/SPL metadata: reflection is classified as unsupported and SPL
    behavior is absent. ID: `E_PHP_IR_UNSUPPORTED_REFLECTION`.
17. Foreach beyond arrays: public-property objects, Iterator,
    IteratorAggregate, generator sources, and invalid-source warnings are
    executable for covered fixtures; temporary by-reference foreach sources and
    the full mutation matrix remain incomplete. IDs:
    `E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH`,
    `E_PHP_RUNTIME_FOREACH_MUTATION_COMPAT`.
18. Constants: runtime `define()`, `defined()`, `constant()`, and
    fixture-covered predefined constants execute; the full PHP 8.5
    constant-expression matrix remains incomplete. ID:
    `E_PHP_RUNTIME_CONST_EXPR_MATRIX`.
19. Warning and fatal text compatibility: VM emits structured diagnostics
    instead of PHP CLI wording. ID: `E_PHP_RUNTIME_WARNING_CHANNEL_COMPAT`.
20. Zend ABI, opcache/JIT, resources, and stream wrappers are intentionally
    deferred beyond this runtime subset. IDs:
    `E_PHP_RUNTIME_UNSUPPORTED_ZEND_ABI`,
    `E_PHP_RUNTIME_UNSUPPORTED_JIT`,
    `E_PHP_RUNTIME_UNSUPPORTED_STREAM_WRAPPER`.

## Runtime/VM Hardening Audit

Scope checked: `crates/php_runtime`, `crates/php_vm`, `crates/php_vm_cli`,
`crates/php_ir`, and runtime testkit paths.

| Class | Current occurrences | Classification |
| --- | --- | --- |
| `expect("frame was pushed")`, `expect("caller frame is active")` in `crates/php_vm/src/vm/mod.rs` | repeated dispatch invariants after frame setup | internal VM invariant; should become controlled diagnostics only if a reachable malformed-state fixture appears |
| `expect("target bounds checked")` in `crates/php_vm/src/frame.rs` | frame target mutation after prior bounds check | internal invariant guarded by VM code |
| Builtin `expect("checked arity")` | builtin implementations after registry arity validation | internal invariant; arity errors are surfaced before this point |
| `panic!("expected server array")` in runtime context tests/support path | protects controlled superglobal construction invariant | not user PHP input reachable in normal VM execution |
| Test and snapshot `unwrap`/`expect` | Rust tests, fixtures, snapshot serialization | acceptable test assertions |
| Testkit `panic!("{reason}")` | reference smoke test failure path | test-only failure reporting |

No `TODO runtime` marker currently indicates a silent runtime branch that
pretends to execute unsupported PHP. Unsupported runtime behavior is expected
to use known-gap IDs, diagnostics, or planned/deferred rows in
`docs/runtime/known-gaps.md`.

## Known-Gap Coverage Status

Known gaps are tracked in `docs/runtime/known-gaps.md`. The final Runtime gate
requires representative fixture files for executable known-gap categories such
as generators, `yield from`, fibers, eval, autoload, reflection, traits,
enums, property hooks, reference categories, foreach by reference, `$GLOBALS`
aliasing, clone-with visibility/readonly, and catch types.

Rows marked `planned` or `deferred` keep explicit examples or scope notes.
They are not counted as implemented and must gain fixtures when Runtime semantics starts
work on that behavior.

## Runtime Semantics Deferred Scope

1. References and Copy-on-Write: replace the local-alias subset with PHP-like
   zval/reference storage for variables, parameters, returns, array elements,
   object properties, and closure captures.
2. Arrays complete: implement full key normalization, spread/unpack, COW,
   element references, sorting/order edge cases, invalid-key diagnostics, and
   array/reference `var_dump` behavior.
3. Objects complete: add visibility, inheritance, interfaces, traits, enums,
   readonly/asymmetric visibility, property hooks, magic methods, dynamic
   class/property/method lookup, `__clone`, and late static binding.
4. Generators: execute `yield`, `yield from`, generator return values, send,
   throw, close, and foreach integration.
5. Fibers: model suspend/resume, scheduling state, errors, and interaction
   with exceptions and generators.
6. Standard library basis: expand selected builtins into a compatibility
   matrix with arity/type behavior and warning/error surfaces.
7. Reflection and SPL: expose runtime metadata, class/function inspection,
   iterator interfaces, core SPL containers, and autoload-sensitive behavior.
8. PHPT expansion: grow local PHPT smoke coverage, classify skips and known
   gaps explicitly, and run reference comparisons where stable.
9. Composer/framework smoke coverage: Runtime semantics has offline, hand-written
   real-world fixtures in `fixtures/runtime_semantics/real_world/`, but real Composer
   package execution remains Standard library work. Required Standard library pieces include
   broader predefined constants, common stdlib helpers (`array_map`, `count`,
   `is_subclass_of`, string/path helpers), Composer PSR-style autoload
   behavior through `class_exists` and unresolved type references, richer
   Reflection construction APIs such as `ReflectionClass::newInstanceArgs`,
   fuller SPL interfaces/classes, and exact warning/fatal text compatibility.

## Current Position

Runtime is green for curated runtime fixtures. It is not
Composer-compatible, framework-compatible, Zend-bytecode-compatible, or ABI
compatible. Runtime semantics should treat every row in the deviation list as open until
new implementation, fixtures, and reference-diff evidence prove otherwise.
