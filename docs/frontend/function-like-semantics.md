# Function-Like Semantics

The semantic frontend extends function-like HIR with compile-time metadata. It does not
create runtime closure objects, generator objects, or perform capture-value
evaluation.

## Flags

`FunctionSignature` records `FunctionLikeFlags`:

- `returns_by_ref`
- `is_static`
- `is_generator`
- `has_return_type_void`
- `has_return_type_never`
- `has_tentative_or_deferred_info`
- `this_available`

Arrow functions also record the source span of their expression body when the
CST exposes it.

## Checks

The Semantic frontend frontend reports stable diagnostics for reference-safe cases:

- duplicate closure-use variables
- closure-use variables that duplicate parameters
- returning a value from a `void` function
- explicit returns from `never` functions

Generator-specific return-value rules are deferred. The HIR sets `is_generator`
when `yield` or `yield from` appears directly in a function-like body, excluding
nested function-like bodies. `$this` availability is recorded as metadata only;
PHP 8.5 lint accepts `$this` in static methods and static closures, so the semantic frontend keeps that behavior deferred rather than reporting a semantic diagnostic.

## Fixtures

The fixtures live under `fixtures/semantic/functions/`:

- `return-void-valid.php`
- `return-void-invalid.php`
- `return-never.php`
- `generator.php`
- `closure-use.php`
- `closure-use-duplicate-invalid.php`
- `arrow-function.php`
- `static-closure-this.php`
