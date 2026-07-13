# HIR Model

The Semantic frontend HIR is the semantic frontend's structured, non-lossless
representation. It is suitable for diagnostics, symbol inspection, and later
bytecode or IR planning, but it is not a runtime representation.

## Required Data

- `HirModule`
- arenas for declarations, statements, expressions, types, attributes, and
  constant-expression records
- typed IDs for HIR nodes
- source maps from HIR IDs to CST/source byte ranges
- missing/error nodes for recovery-safe lowering

## Foundation

`php_semantics` now owns the initial HIR/database foundation:

- `hir::ids` defines typed IDs for modules, namespaces, declarations,
  functions, class-likes, methods, properties, constants, parameters,
  expressions, statements, types, attributes, scopes, and symbols. IDs expose
  deterministic raw indexes for snapshots but remain distinct public types.
- `hir::arena::Arena<T, Id>` is an append-only typed arena with allocation,
  lookup, mutable lookup, indexing, and source-order iteration.
- `hir::module::HirModule` stores the source root kind, source byte length, and
  arenas for declarations, statements, expressions, types, attributes, and
  function-like signatures.
- `hir::decl`, `hir::expr`, `hir::stmt`, `hir::types`, `hir::attributes`, and
  `hir::names` contain records that later lowering work items fill without
  introducing runtime values.
- `hir::expr` and `hir::stmt` now contain structural expression and statement
  HIR, including recovery-safe `Missing` nodes and PHP 8.5
  pipe/clone-with/first-class-callable forms.
- `hir::const_expr` records constant-expression candidates with
  context, structural kind, source expression ID, allowed flag, and source map
  span. These records annotate expression HIR; they do not evaluate values.
- `hir::signatures` records function-like signatures, ordered parameters,
  return type references, default-value source references, by-reference and
  variadic flags, and constructor-promotion metadata.
- `hir::class_like` records class-like declarations and member HIR for methods,
  properties, and class constants. Member summaries on class-like records carry
  typed IDs into the detailed member arenas.
- `hir::class_like::HirTraitUse` records trait-use declarations and
  token-derived adaptation entries for `as` and `insteadof` blocks.
- `hir::class_like::HirEnumCase` records enum cases with owning enum IDs,
  optional backing-value constant-expression links, attributes, and source-map
  spans.
- `hir::declare` records `declare(...)` statement metadata and file-level
  directive summaries for `strict_types`, `ticks`, `encoding`, and unknown
  directives without applying runtime effects.
- `db::FrontendDatabase` owns HIR modules and a `SourceMap`.
- `db::SourceMap` maps typed HIR/semantic IDs back to `php_source::TextRange`
  byte spans.
- `FrontendResult` keeps parser diagnostics, semantic diagnostics, the
  `SemanticModule` summary, and the `FrontendDatabase`.

`analyze_source` currently allocates one empty `HirModule` for the parsed source
file and maps its `ModuleId` to the CST root range. Later work items are
responsible for declaration collection, lowering, symbol tables, scope tables,
and diagnostics.

## Boundaries

HIR may record that a construct is deferred to runtime. It must not run PHP
files, resolve autoloaded classes, instantiate attributes, execute includes, or
evaluate general expressions.

`docs/frontend/expression-statement-hir.md` documents the expression
and statement lowering shape, JSON output, and current control-header
placeholder behavior.

## PHP 8.5 Surface

HIR must make PHP 8.5 forms visible enough for downstream consumers and
fixtures:

- pipe operator
- `(void)` cast
- clone-with
- closures in constant-expression contexts
- first-class callables in constant-expression contexts
- casts in constant-expression contexts
- `new` in constant-expression contexts where PHP 8.5.7 accepts it
- property hooks and asymmetric visibility metadata
- class-like declarations with structural member summaries, resolved
  `extends`/`implements`/trait-use names, attributes, and enum backing types
- method/property/class-constant HIR with source maps, modifiers, type links,
  attributes, defaults/initializers, and property-hook summaries
- trait-use declarations with resolved trait references and conservative
  adaptation metadata for aliases and precedence rules
- enum-case declarations with unit/backed value validation and duplicate case
  diagnostics
- method HIR with optional magic-method classification for class-context and
  signature checks
- `declare` metadata with conservative literal directive values and source
  spans
