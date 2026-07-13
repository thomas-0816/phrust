# Declaration Model

Semantic frontend declaration collection builds per-file declarations from AST views and
records enough information for name resolution, diagnostics, snapshots, and
Runtime follow-up.

## Collected Declarations

- namespaces
- imports
- top-level functions
- top-level constants
- classes
- interfaces
- traits
- enums
- anonymous classes
- methods
- properties
- class constants
- enum cases
- parameters
- attributes

## Rules

Declaration collection is per source file. Conditional declarations are marked
instead of executed. Include, require, eval, and autoload-sensitive effects are
represented as deferred effects, not as additional loaded declarations.

Duplicate and invalid declarations produce semantic diagnostics with stable
IDs. Parser acceptance must remain comparable with the PHP lint oracle.

## Namespace Collection

The first declaration-collection pass lives in
`php_semantics::lower::declarations`:

- `LoweringContext` owns a `DiagnosticReporter`.
- `collect_module_declarations` walks the `php_ast::SourceFile` view and
  populates `HirModule::namespaces`.
- `HirNamespaceBlock` records optional namespace name, namespace form, source
  span, and coarse `TopLevelItem` entries.
- `NamespaceForm` distinguishes synthetic global, braced, and unbraced
  namespaces.
- `TopLevelItemKind` currently records inline HTML, `declare`, imports,
  constants, functions, class/interface/trait/enum declarations, executable
  statements, and unknown recovery cases.
- `FrontendResult` JSON includes namespace summaries under
  `module.namespaces`.

The pass validates the first namespace-structure rules needed by later
declaration collection:

- A namespace declaration may follow initial `declare` statements.
- Non-`declare` PHP code before the first namespace declaration emits
  `E_PHP_NAMESPACE_MUST_BE_FIRST_STATEMENT`.
- Mixing braced and unbraced namespace declarations emits
  `E_PHP_MIXED_NAMESPACE_DECLARATIONS`.
- Top-level PHP code outside a braced namespace after explicit braced
  namespaces is diagnosed.
- Inline HTML is preserved as a global top-level item instead of being
  reinterpreted as PHP code.

This pass does not execute top-level code.

## Declaration Registration

`php_semantics::symbols::declarations` defines the per-source-file declaration
table:

- `DeclarationTable` stores declarations in deterministic registration order.
- `DeclarationKind` distinguishes functions, constants, classes, interfaces,
  traits, enums, conditional functions, and conditional class-like
  declarations.
- Every registered declaration receives a stable-in-run `DeclId` and matching
  `SymbolId`; both are mapped to source byte spans through `SourceMap`.
- FQNs are constructed from the current namespace plus the declaration short
  name. Imports do not rename declarations.
- Unconditional declarations are checked for safe same-file duplicates in the
  relevant PHP name namespace. Conditional declarations are recorded but not
  treated as always-available globals, because their existence depends on
  runtime execution.

The CLI exposes this table with `php-frontend symbols <file>` in text or JSON
form. JSON analysis also includes the declaration table under `module.symbols`.

Cross-file duplicate checks, autoloading, include/require side effects, and
conditional declaration execution are outside Semantic frontend.
