```text
You are working in `mayflower/phrust` on a dedicated branch from current `main`.

Goal:
Move actual PHP behavior toward PHP 8.5.7 PHPT parity. Do not add bookkeeping-only changes unless they directly support implementation and verification.

Hard rules:
- Use php-src as the read-only oracle: Original PHPTs, Reference PHP output, stubs/arginfo, and source lookup notes.
- Do not edit `third_party/php-src/`.
- Do not commit `target/`.
- Do not implement a web/FPM/CGI/Apache SAPI.
- Do not touch tokenizer; tokenizer is already implemented.
- Do not hide failures by policy changes. Improve behavior or document a concrete remaining gap.
- Do not create god files or implementation-history modules.
- No new stubs unless the prompt explicitly allows a temporary negative/unsupported path.
- Every behavior fix needs a focused fixture: upstream PHPT selected in a module manifest or a generated/minimized PHPT with provenance.
- Do not set `PHPT_ACCEPT_BASELINE=1` unless explicitly instructed.
- Keep public APIs stable unless the prompt explicitly authorizes a migration.
- PHP oracle is in /Volumes/CrucialMusic/src/phrust/third_party/php-src

```

Purpose: implement the object/callable backbone needed by PHP frameworks and many standard functions.

This branch owns:

```text
crates/php_runtime/src/object/**
crates/php_runtime/src/callable.rs
crates/php_runtime/src/types.rs
crates/php_std/src/arginfo.rs
crates/php_std/src/generated/**
scripts/stdlib/generate_arginfo.py
crates/php_ir/src/lower.rs              # only calls/functions/objects/class lowering
crates/php_ir/src/module.rs             # only class/function metadata if needed
crates/php_ir/src/instruction.rs        # only calls/objects/class instructions if needed
crates/php_vm/src/**                    # only calls/functions/objects/classes/exceptions/reflection execution
crates/php_runtime/src/builtins/modules/spl.rs
crates/php_runtime/src/builtins/modules/reflection.rs
docs/phpt/modules/zend.functions.md
docs/phpt/modules/objects.classes.md
docs/phpt/modules/spl.md
docs/phpt/modules/reflection.md
tests/phpt/generated/zend.functions/**
tests/phpt/generated/objects.classes/**
tests/phpt/generated/spl/**
tests/phpt/generated/reflection/**
```

Avoid: scalar/array/string builtins unless strictly needed for callable/object fixtures.

## Prompt 3A — generated arginfo becomes mandatory infrastructure

```text
You are on branch `phpt/impl-functions-objects-reflection`.

Goal:
Make generated arginfo real and usable for builtin arity/type/reflection.

Tasks:
1. Run:
   - `nix develop -c just generate-arginfo`
2. Ensure generated arginfo is non-empty and committed in the intended generated location.
3. Minimum metadata:
   - function name
   - extension/module
   - parameter names
   - required/optional
   - variadic
   - by-ref
   - simple type atoms
   - nullable
   - simple defaults
4. Add a test that fails if generated arginfo is empty.
5. Wire `php_std::arginfo` consumers to use generated metadata.
6. Do not implement function behavior here.

Acceptance:
- `nix develop -c just generate-arginfo`
- `nix develop -c cargo test -p php_std`
- `nix develop -c just verify-stdlib`

End report:
- imported function/class/method counts.
- unsupported signature features.
```

## Prompt 3B — function calls, arity, defaults, variadics

```text
You are on branch `phpt/impl-functions-objects-reflection`, after Prompt 3A.

Goal:
Implement user-function argument semantics and builtin arity against PHPTs.

Scope:
- user functions
- required args
- optional defaults
- extra args visible to `func_get_args`
- variadics
- named args if already represented cleanly
- builtin arity from arginfo
- `ArgumentCountError`-like behavior

Tasks:
1. Run:
   - `nix develop -c just phpt-dev-module MODULE=zend.functions`
2. Implement only call preparation and arity/default/variadic behavior.
3. Add generated PHPTs:
   - too few args
   - extra args
   - defaults
   - variadic packing
   - builtin too few/too many args
4. Update:
   - `docs/phpt/reports/zend.functions-current.md`

Acceptance:
- `nix develop -c cargo test -p php_ir`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.functions`
- `nix develop -c just verify-runtime`

End report:
- Function call cases passing.
- Remaining callable/type blockers.
```

## Prompt 3C — callables and Closure class

```text
You are on branch `phpt/impl-functions-objects-reflection`, after Prompt 3B.

Goal:
Implement real callable/Closure behavior needed by PHPTs.

Scope:
- closures
- first-class callables
- callable arrays
- `Closure` internal class
- `instanceof Closure`
- `Closure::fromCallable`
- callable invocation
- `is_callable` for covered forms

Non-scope:
- full `Closure::bind`/`bindTo` unless selected PHPTs require and object model supports it.
- source-string matching.

Tasks:
1. Run:
   - `nix develop -c just phpt-dev-module MODULE=zend.functions`
2. Implement callable acquisition through one unified path.
3. Add internal `Closure` class metadata.
4. Add generated PHPTs:
   - closure invocation
   - first-class callable function
   - first-class callable method
   - callable array valid/invalid
   - `Closure::fromCallable`
5. Update known gaps.

Acceptance:
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=zend.functions`

End report:
- Callable forms supported.
- Closure methods unsupported.
```

## Prompt 3D — object/class basics

```text
You are on branch `phpt/impl-functions-objects-reflection`, after Prompt 3C.

Goal:
Implement class construction, properties, methods, and visibility basics.

Scope:
- class lookup
- `new C`
- `__construct`
- `$this`
- public property read/write
- typed property initialization
- public method call
- private/protected visibility errors
- static methods/properties MVP

Tasks:
1. Run:
   - `nix develop -c just phpt-dev-module MODULE=objects.classes`
2. Fix basic object execution before magic behavior.
3. Add generated PHPTs:
   - constructor property
   - method call
   - `$this`
   - public/private/protected property
   - typed property uninitialized
   - static property/method
4. Route invalid access to PHP-like catchable `Error` behavior where infrastructure supports it.

Acceptance:
- `nix develop -c cargo test -p php_runtime object`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=objects.classes`
- `nix develop -c just verify-runtime`

End report:
- Object basics before/after.
- Remaining object gaps.
```

## Prompt 3E — magic methods, clone, traits, enums

```text
You are on branch `phpt/impl-functions-objects-reflection`, after Prompt 3D.

Goal:
Implement framework-relevant object semantics after basics are stable.

Scope:
- `__get`
- `__set`
- `__isset`
- `__unset`
- `__call`
- `__callStatic`
- `__invoke`
- `__toString`
- clone
- `__clone`
- PHP 8.5 clone-with public properties
- trait method composition MVP
- unit/backed enum MVP

Non-scope:
- full property-hook matrix
- full readonly/asymmetric visibility matrix
- serialization magic unless selected PHPTs require it.

Tasks:
1. Run:
   - `nix develop -c just phpt-dev-module MODULE=objects.classes`
2. Implement behavior in small vertical slices.
3. Add PHPTs for each supported magic method and enum/trait case.
4. Keep unsupported cases with stable known-gap IDs.

Acceptance:
- `nix develop -c cargo test -p php_runtime object`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=objects.classes`

End report:
- Magic/clone/trait/enum coverage.
- Remaining gaps.
```

## Prompt 3F — SPL MVP

```text
You are on branch `phpt/impl-functions-objects-reflection`, after Prompt 3E.

Goal:
Implement SPL MVPs needed by framework and common library code.

Scope:
- Countable
- Iterator
- IteratorAggregate
- ArrayAccess
- ArrayIterator
- IteratorIterator
- EmptyIterator
- LimitIterator
- ArrayObject
- SplFixedArray
- SplObjectStorage
- SplDoublyLinkedList
- SplStack
- SplQueue

Non-scope:
- SplFileObject; that belongs to filesystem branch.
- full serialization/flags matrices.

Tasks:
1. Run:
   - `nix develop -c just phpt-dev-module MODULE=spl`
2. Implement classes using runtime object/internal class infrastructure.
3. Route iteration through VM foreach path.
4. Add generated PHPTs per class.
5. Update SPL docs/reports.

Acceptance:
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=spl`
- `nix develop -c just diff-spl-reflection`

End report:
- SPL classes implemented.
- Remaining SPL gaps.
```

## Prompt 3G — Reflection MVP

```text
You are on branch `phpt/impl-functions-objects-reflection`, after Prompt 3F.

Goal:
Expose real metadata through Reflection APIs.

Scope:
- ReflectionFunction
- ReflectionParameter
- ReflectionClass
- ReflectionMethod
- ReflectionProperty
- ReflectionAttribute
- ReflectionEnum if enum MVP exists
- internal function metadata from arginfo
- userland metadata from semantic/IR/runtime tables

Non-scope:
- full ReflectionExtension if extension metadata is not stable.
- Reflection invocation APIs unless PHPT-selected and safe.

Tasks:
1. Run:
   - `nix develop -c just phpt-dev-module MODULE=reflection`
2. Use generated arginfo and existing metadata.
3. Do not invent fake metadata.
4. Add generated PHPTs:
   - builtin function parameters
   - user function parameters
   - class/method/property modifiers
   - attributes
   - enums if available
5. Update:
   - `docs/phpt/modules/reflection.md`
   - `docs/phpt/reports/reflection-current.md`

Acceptance:
- `nix develop -c cargo test -p php_std`
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=reflection`
- `nix develop -c just diff-spl-reflection`

End report:
- Reflection APIs implemented.
- Remaining Reflection gaps.
```

## Prompt 3H — branch closeout

```text
You are on branch `phpt/impl-functions-objects-reflection`.

Goal:
Close functions/objects/SPL/Reflection branch.

Tasks:
1. Run:
   - `nix develop -c just verify-runtime`
   - `nix develop -c just verify-stdlib`
   - `nix develop -c just phpt-dev-module MODULE=zend.functions`
   - `nix develop -c just phpt-dev-module MODULE=objects.classes`
   - `nix develop -c just phpt-dev-module MODULE=spl`
   - `nix develop -c just phpt-dev-module MODULE=reflection`
2. Update:
   - `docs/phpt/reports/functions-objects-reflection-summary.md`

End report:
- Before/after for all owned modules.
- Remaining blockers.
- Merge risks.
```
