# Modifier Model

The semantic frontend provides a semantic modifier layer without moving PHP modifier semantics
into the parser.

## HIR Surface

`php_semantics::hir::ModifierSet` normalizes source modifier tokens into:

- visibility: `public`, `protected`, `private`
- property set visibility: `public(set)`, `protected(set)`, `private(set)`
- flags: `abstract`, `final`, `static`, `readonly`, `var`, by-reference,
  variadic, promoted, and hook-related

Byte spans remain source-of-truth for diagnostics.

## Validation

`checks::modifiers` validates direct modifiers on class-like declarations,
methods, class constants, properties, and parameters. It reports duplicate
modifiers, multiple visibility groups, invalid target contexts, abstract/final
combinations, abstract private methods, static readonly properties, and
asymmetric property set-visibility ordering.

Property hooks are recognized as hook-related property members but are not
executed or lowered into runtime behavior in Semantic frontend.

## Fixtures

- `fixtures/semantic/modifiers/class-valid.php`
- `fixtures/semantic/modifiers/class-invalid.php`
- `fixtures/semantic/modifiers/method-valid.php`
- `fixtures/semantic/modifiers/method-invalid.php`
- `fixtures/semantic/modifiers/property-valid.php`
- `fixtures/semantic/modifiers/property-invalid.php`
- `fixtures/semantic/modifiers/asymmetric-visibility.php`
- `fixtures/semantic/modifiers/property-hooks.php`
