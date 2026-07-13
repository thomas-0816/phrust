# Class Context Checks

The semantic frontend provides semantic checks that are decidable from the current source file
and class-like nesting. The pass does not autoload, execute code, or resolve
parent classes across files.

## Context Stack

The checker tracks class-like nesting directly over the CST:

- classes and anonymous classes allow `parent` only when an `extends` clause is
  present
- interfaces and enums do not allow `parent`
- traits allow `parent` because the consuming class determines the parent at
  runtime

`self`, `parent`, and contextual `static` are checked in names, type positions,
`static::`, and `new static()` forms. Invalid context diagnostics use source
spans from the keyword token.

## Deferred Runtime Behavior

PHP 8.5 lint accepts `$this` inside static methods and static closures. Semantic frontend
therefore records `$this` availability on function-like signatures but does not
emit a diagnostic for `$this` use in those contexts.

## Magic Methods

`HirMethod` records an optional `magic_kind` for recognized magic method names:

- `__construct`
- `__destruct`
- `__call`
- `__callStatic`
- `__get`
- `__set`
- `__isset`
- `__unset`
- `__sleep`
- `__wakeup`
- `__serialize`
- `__unserialize`
- `__toString`
- `__invoke`
- `__set_state`
- `__clone`
- `__debugInfo`

The checker only reports reference-confirmed compile-time rules: required
static or non-static shape for magic methods and exact parameter counts where
PHP lint rejects mismatches.

## Fixtures

The fixtures live under `fixtures/semantic/classes/`:

- `self-parent-static-valid.php`
- `self-outside-class-invalid.php`
- `parent-without-parent.php`
- `this-static-method.php`
- `magic-methods-valid.php`
- `magic-methods-invalid.php`
