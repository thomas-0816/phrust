# Enum Semantics

The semantic frontend lowers PHP enum cases into dedicated HIR records without executing
enum behavior or resolving autoload-sensitive class facts.

## HIR

- `HirClassLike` records enum declarations as `ClassLikeKind::Enum`.
- Backed enums link their `int` or `string` backing type through the existing
  type arena.
- Each `case` declaration is lowered into `HirEnumCase`.
- Class-like member summaries link enum cases through
  `ClassLikeMemberId::EnumCase`.
- `HirEnumCase::value` links to the constant-expression candidate for backed
  values when one is present.
- Attributes on enum cases are linked through the shared attribute arena.
- Source maps cover each enum-case declaration span.

## Diagnostics

The semantic frontend currently checks structural enum rules that do not require
runtime execution:

- unit enum cases must not declare backing values
- backed enum cases must declare backing values
- duplicate enum case names are diagnosed within the owning enum

Backing-value type compatibility and duplicate backing values remain deferred:
they require value-level checks beyond the current Semantic frontend constant-expression
surface.

## Fixtures

Enum fixtures live under `fixtures/semantic/enums/`:

- `unit.php`
- `backed.php`
- `attributes.php`
- `unit-case-value-invalid.php`
- `backed-case-missing-invalid.php`
- `duplicate-case-invalid.php`

The fixture runner should emit stable frontend JSON containing both
`class_likes` and `enum_cases`. The semantic diff compares accept/reject status
against the PHP 8.5.7 reference when `REFERENCE_PHP` is available, and otherwise
skips reference-dependent comparison clearly.
