# PHP Name Resolution

Semantic frontend implements PHP source-level name normalization and import resolution
for a single analyzed file.

## Name Model

`php_semantics::hir::names` defines the source-level name representation used by
later resolver passes:

- `RawName` preserves the non-trivia spelling reconstructed from the existing
  CST/AST name tokens.
- `QualifiedName` records the original spelling, semantic name parts, a leading
  slash marker for `\Foo\Bar`, and a `namespace\Foo` relative marker.
- `FullyQualifiedName` stores resolved FQN parts without a leading slash marker.
- `NamePart` keeps both the original part spelling and an ASCII-only lowercase
  spelling. This intentionally follows the lexer/parser byte model and does not
  add Unicode case-folding semantics.
- `php_semantics::symbols::NameInterner` interns names by `NameKind` plus a
  prefix-preserving canonical key.

The AST lowering entry point is `QualifiedName::from_ast_name(php_ast::Name)`.
It uses the existing lexer/CST tokens and does not introduce a second lexer.

## Case Rules

Class, interface, trait, and enum names are looked up practically
case-insensitively by PHP. The frontend preserves the first source spelling but
uses ASCII lowercase canonical spelling for `NameKind::ClassLike`.

Function names use case-insensitive global lookup. Namespace-local lookup,
function-import handling, and runtime fallback are resolver concerns layered on
top of this model. `NameKind::Function` therefore also uses ASCII
lowercase canonical spelling.

Constants have their own rules. User-defined constants are not folded in this
base interner, and magic constants or dynamic constant access are not normal
source names in this pipeline.

Variable names are case-sensitive and intentionally separate from class,
function, namespace, and constant resolution. Variable identifiers should only
use `NameKind::VariableName` when a later pass explicitly needs to intern them.

Names written as `namespace\Foo` are marked namespace-relative and store `Foo`
as the semantic part. Names written as `\Foo\Bar` are marked fully-qualified
and store `Foo`, `Bar` as the FQN parts.

## Inputs

- current namespace
- grouped and ungrouped `use` declarations
- class/function/const import kind
- fully qualified, relative, and unqualified names
- contextual names such as `self`, `parent`, and `static`

## Outputs

- resolved fully qualified names where compile-time resolution is defined
- deferred runtime fallback markers for function and constant lookup
- duplicate import alias diagnostics
- invalid namespace/import diagnostics

## Non-Goals

- no autoload execution
- no class existence checks across files
- no include/eval side effects
- no runtime fallback execution
