# Declare Directives

The semantic frontend records `declare(...)` statements as frontend metadata. The semantic
frontend does not apply runtime behavior for `strict_types`, `ticks`, or
`encoding`.

## HIR

`HirDeclare` stores each `declare` statement with source span and ordered
`DeclareDirective` entries. Each directive records:

- source spelling of the directive name
- canonical lowercase name
- conservative literal value as `int`, `string`, or `unknown`
- directive and value byte spans

`FileDirectives` summarizes the last observed `strict_types`, `encoding`, and
`ticks` directive and preserves unknown directives in source order. The
`FrontendResult` JSON includes both the per-file summary as `file_directives`
and the ordered `declares` array.

## Checks

The Semantic frontend checker only reports reference-confirmed compile-time failures:

- `declare(strict_types=...)` must use integer literal `0` or `1`.
- `declare(strict_types=...)` must be the first statement in the script.

Duplicate directives in one `declare` are metadata-only because the reference
lint oracle accepts them. `ticks` and `encoding` values are recorded
conservatively and are not evaluated.

## Boundaries

`strict_types` runtime coercion behavior is deferred to Runtime. `ticks` does
not install handlers, and `encoding` does not change source decoding.
