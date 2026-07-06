# Semantic Frontend Validation Summary

The semantic frontend consumes the parser CST through typed AST views and
produces HIR, declarations, scopes, symbols, semantic diagnostics, and deferred
runtime metadata for PHP 8.5.7 compatibility work.

## Current Scope

### `php_ast`

`php_ast` provides typed CST views over `php_syntax`. It exposes node/token
wrappers, declarations, statements, expressions, class-like views, attributes,
name/type helpers, source-local AST pointers, and validation helpers. It does
not introduce a second lexer or parser.

### `php_semantics`

`php_semantics` owns:

- semantic database and module model;
- typed HIR IDs and arenas;
- declaration, import, symbol, scope, type, attribute, and constant-expression
  metadata;
- name-resolution records with deferred runtime fallback metadata;
- semantic diagnostics with stable IDs, spans, notes, deduplication, and a
  diagnostic cap;
- HIR lowering with missing-node recovery;
- query-shaped frontend APIs.

The high-level API is `php_semantics::query::frontend::analyze_file`, returning
`FrontendResult`.

### `php_frontend_cli`

`php_frontend_cli` consumes `php_semantics` and exposes:

- `analyze`
- `diagnostics`
- `symbols`
- `scopes`
- `hir`
- `snapshot`

Supported options include `--format text|json`, `--php-version-target 8.5`,
`--show-spans`, `--show-source-map`, `--show-deferred`,
`--fail-on-diagnostics`, and `--pretty`.

Exit codes are:

- 0: success
- 1: I/O error
- 2: usage error
- 3: diagnostics with `--fail-on-diagnostics`

## Validation

Use the frontend gate when changing typed AST views, HIR, declarations, scopes,
semantic diagnostics, frontend snapshots, or the frontend CLI:

```bash
nix develop -c just verify-frontend
```

Focused checks include:

```bash
nix develop -c just semantic-fixtures
nix develop -c just semantic-diff
nix develop -c just frontend-snapshots
```

Optional corpus, fuzz, and benchmark checks are explicit soft gates:
`semantic-corpus-smoke`, `fuzz-frontend-smoke`, and `bench-frontend`.

## Diagnostic ID Inventory

- `PHS0000`
- `E_PHP_DUPLICATE_PARAMETER`
- `E_PHP_VARIADIC_PARAMETER_NOT_LAST`
- `E_PHP_INVALID_PARAMETER_DEFAULT`
- `E_PHP_INVALID_PROPERTY_PROMOTION`
- `E_PHP_CLOSURE_USE_DUPLICATES_PARAMETER`
- `E_PHP_DUPLICATE_CLOSURE_USE_VARIABLE`
- `E_PHP_HIR_MISSING_CHILD`
- `E_PHP_DUPLICATE_USE_ALIAS`
- `E_PHP_DUPLICATE_DECLARATION`
- `E_PHP_MIXED_NAMESPACE_DECLARATIONS`
- `E_PHP_NAMESPACE_MUST_BE_FIRST_STATEMENT`
- `E_PHP_INVALID_TYPE_VOID_CONTEXT`
- `E_PHP_INVALID_TYPE_NEVER_CONTEXT`
- `E_PHP_INVALID_TYPE_STATIC_CONTEXT`
- `E_PHP_INVALID_TYPE_SELF_CONTEXT`
- `E_PHP_INVALID_TYPE_PARENT_CONTEXT`
- `E_PHP_INVALID_TYPE_CALLABLE_CONTEXT`
- `E_PHP_DUPLICATE_TYPE_ALTERNATIVE`
- `E_PHP_DUPLICATE_MODIFIER`
- `E_PHP_INCOMPATIBLE_MODIFIERS`
- `E_PHP_BREAK_NOT_IN_LOOP_OR_SWITCH`
- `E_PHP_CONTINUE_NOT_IN_LOOP_OR_SWITCH`
- `E_PHP_INVALID_BREAK_CONTINUE_LEVEL`
- `E_PHP_RETURN_OUTSIDE_ALLOWED_CONTEXT`
- `E_PHP_RETURN_VALUE_FROM_VOID_FUNCTION`
- `E_PHP_RETURN_FROM_NEVER_FUNCTION`
- `E_PHP_YIELD_OUTSIDE_FUNCTION`
- `E_PHP_GOTO_LABEL_NOT_FOUND`
- `E_PHP_INVALID_CONST_EXPR`
- `E_PHP_ATTRIBUTE_ARGUMENT_NOT_CONST_EXPR`
- `E_PHP_DUPLICATE_CLASS_MEMBER`
- `E_PHP_ENUM_CASE_VALUE_ON_UNIT_ENUM`
- `E_PHP_ENUM_CASE_MISSING_VALUE_ON_BACKED_ENUM`
- `E_PHP_TRAIT_ADAPTATION_INVALID_SHAPE`
- `E_PHP_INVALID_CLASS_CONTEXT_NAME`
- `E_PHP_INVALID_MAGIC_METHOD_SIGNATURE`
- `E_PHP_INVALID_STRICT_TYPES_DECLARE`
- `E_PHP_STRICT_TYPES_DECLARE_NOT_FIRST`
- `E_PHP_INVALID_VOID_CAST`
- `W_PHP_REFERENCE_BEHAVIOR_DEFERRED`
- `N_PHP_RUNTIME_CHECK_DEFERRED`

## Coverage

- Namespaces: braced/unbraced namespace blocks, placement checks, and mixed
  namespace-form diagnostics.
- Imports: class, function, const, grouped imports, and alias collision checks.
- Declarations: functions, constants, class-like declarations, conditional
  declaration metadata, and same-file duplicate checks.
- Scopes: file, namespace, function, method, closure, arrow function,
  global/static statements, and closure-use metadata.
- Types: unions, intersections, DNF types, nullable forms, and contextual
  invalid `void`, `never`, `static`, `self`, `parent`, and `callable` cases.
- Constant expressions: scalar and array forms, class constant fetches,
  conservative literal folding, invalid variable/call checks, and PHP 8.5
  closure/new/cast/first-class callable fixtures.
- Attributes: target metadata, argument constant-expression validation,
  repeated attributes, and class/function/method/parameter/property/enum
  coverage.
- Class-like constructs: classes, interfaces, traits, enums, properties,
  methods, constants, modifiers, property hooks, constructor promotion, trait
  adaptations, enum cases, and magic-method diagnostics.

## Known Gaps

- Full cross-file symbol linking and autoload-aware resolution are deferred
  metadata, not executed by the semantic frontend.
- Include, require, and eval effects are recorded conservatively and not
  executed.
- Exact PHP fatal-message wording is not guaranteed; diagnostics compare stable
  IDs, spans, severity, and acceptance behavior.
- Full CFG-level `goto` boundary validation remains a known gap.

Runtime and VM work should consume
`php_semantics::query::frontend::analyze_file` and keep parser diagnostics,
semantic diagnostics, and runtime diagnostics separate.
