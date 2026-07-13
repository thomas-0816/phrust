# Compile-Time Diagnostics

Semantic diagnostics are separate from parser diagnostics and use stable IDs.
They are designed for fixture snapshots, reference comparison, and future IDE
translation.

## Fields

- diagnostic ID
- severity
- phase
- message
- primary byte span
- optional labels
- optional related information

## Initial Phases

- declaration collection
- name resolution
- type lowering
- HIR lowering
- constant expression validation
- attribute lowering
- modifier validation
- control-flow validation
- class-like validation

Exact PHP error text compatibility is not required for Semantic frontend. Acceptance,
diagnostic category, source location, and known-gap classification are the
primary comparison points.

## Diagnostic System

`php_semantics::diagnostics` defines the stable diagnostic surface:

- `SemanticDiagnostic` with ID, severity, phase, message, optional primary
  span, labels, and notes.
- `DiagnosticLabel` for secondary source spans.
- `DiagnosticReporter` for collecting diagnostics without requiring every
  caller to allocate a `Vec` directly.
- `DiagnosticId`, `DiagnosticSeverity`, and `DiagnosticPhase` with stable JSON
  names.

JSON diagnostics are emitted as objects with this shape:

```json
{
  "id": "E_PHP_DUPLICATE_USE_ALIAS",
  "severity": "error",
  "phase": "declaration_collection",
  "message": "duplicate import alias",
  "span": { "start": 10, "end": 15 },
  "labels": [
    { "message": "previous alias is here", "span": { "start": 1, "end": 5 } }
  ],
  "notes": ["aliases are compared case-insensitively where PHP does"]
}
```

When no primary span is available, `span` is `null`; diagnostics must still be
renderable and must not panic.

## Initial Diagnostic IDs

| ID | Severity | Phase | Reference mapping |
| --- | --- | --- | --- |
| `PHS0000` | note | any semantic layer | Reserved marker for future semantic diagnostics |
| `E_PHP_DUPLICATE_PARAMETER` | error | declaration collection | PHP compile-time duplicate parameter fatal error |
| `E_PHP_DUPLICATE_USE_ALIAS` | error | declaration collection | PHP compile-time duplicate import alias fatal error |
| `E_PHP_DUPLICATE_DECLARATION` | error | declaration collection | Semantic frontend same-file duplicate declaration check |
| `E_PHP_MIXED_NAMESPACE_DECLARATIONS` | error | declaration collection | PHP namespace declaration form fatal error |
| `E_PHP_NAMESPACE_MUST_BE_FIRST_STATEMENT` | error | declaration collection | PHP namespace placement fatal error |
| `E_PHP_INVALID_TYPE_VOID_CONTEXT` | error | type lowering | PHP invalid `void` type context fatal error |
| `E_PHP_INVALID_TYPE_NEVER_CONTEXT` | error | type lowering | PHP invalid `never` type context fatal error |
| `E_PHP_INVALID_TYPE_STATIC_CONTEXT` | error | type lowering | PHP invalid `static` type context fatal error |
| `E_PHP_INVALID_TYPE_SELF_CONTEXT` | error | type lowering | PHP invalid `self` type context fatal error |
| `E_PHP_INVALID_TYPE_PARENT_CONTEXT` | error | type lowering | PHP invalid `parent` type context fatal error |
| `E_PHP_INVALID_TYPE_CALLABLE_CONTEXT` | error | type lowering | PHP invalid `callable` type context fatal error |
| `E_PHP_DUPLICATE_TYPE_ALTERNATIVE` | error | type lowering | PHP duplicate union/intersection type fatal error |
| `E_PHP_HIR_MISSING_CHILD` | error | HIR lowering | Recovery CST child missing while constructing structural HIR |
| `E_PHP_VARIADIC_PARAMETER_NOT_LAST` | error | HIR lowering | PHP variadic parameter ordering fatal error |
| `E_PHP_INVALID_PARAMETER_DEFAULT` | error | HIR lowering | PHP invalid parameter default fatal error |
| `E_PHP_INVALID_PROPERTY_PROMOTION` | error | HIR lowering | PHP invalid constructor property promotion fatal error |
| `E_PHP_CLOSURE_USE_DUPLICATES_PARAMETER` | error | HIR lowering | PHP closure use duplicate parameter fatal error |
| `E_PHP_DUPLICATE_CLOSURE_USE_VARIABLE` | error | HIR lowering | PHP duplicate closure use variable fatal error |
| `E_PHP_INVALID_VOID_CAST` | error | HIR lowering | Pinned PHP reference rejects `(void)` cast syntax |
| `E_PHP_INVALID_STRICT_TYPES_DECLARE` | error | declaration collection | PHP invalid `declare(strict_types=...)` value fatal error |
| `E_PHP_STRICT_TYPES_DECLARE_NOT_FIRST` | error | declaration collection | PHP `strict_types` placement fatal error |
| `E_PHP_DUPLICATE_MODIFIER` | error | modifier validation | PHP duplicate modifier fatal error |
| `E_PHP_INCOMPATIBLE_MODIFIERS` | error | modifier validation | PHP incompatible modifier fatal error |
| `E_PHP_BREAK_NOT_IN_LOOP_OR_SWITCH` | error | control-flow validation | PHP compile-time `break` context fatal error |
| `E_PHP_CONTINUE_NOT_IN_LOOP_OR_SWITCH` | error | control-flow validation | PHP compile-time `continue` context fatal error |
| `E_PHP_INVALID_BREAK_CONTINUE_LEVEL` | error | control-flow validation | PHP invalid `break`/`continue` level fatal error |
| `E_PHP_RETURN_OUTSIDE_ALLOWED_CONTEXT` | error | control-flow validation | PHP invalid `return` context fatal error |
| `E_PHP_RETURN_VALUE_FROM_VOID_FUNCTION` | error | control-flow validation | PHP return-from-void fatal error |
| `E_PHP_RETURN_FROM_NEVER_FUNCTION` | error | control-flow validation | PHP explicit return from `never` function fatal error |
| `E_PHP_YIELD_OUTSIDE_FUNCTION` | error | control-flow validation | PHP yield context compile-time fatal error |
| `E_PHP_GOTO_LABEL_NOT_FOUND` | error | control-flow validation | PHP missing goto label fatal error |
| `E_PHP_INVALID_CONST_EXPR` | error | constant expression | PHP invalid constant expression fatal error |
| `E_PHP_ATTRIBUTE_ARGUMENT_NOT_CONST_EXPR` | error | constant expression | PHP attribute argument constant-expression fatal error |
| `E_PHP_DUPLICATE_CLASS_MEMBER` | error | class-like validation | PHP duplicate class member fatal error |
| `E_PHP_ENUM_CASE_VALUE_ON_UNIT_ENUM` | error | class-like validation | PHP enum case value fatal error |
| `E_PHP_ENUM_CASE_MISSING_VALUE_ON_BACKED_ENUM` | error | class-like validation | PHP backed enum missing value fatal error |
| `E_PHP_TRAIT_ADAPTATION_INVALID_SHAPE` | error | class-like validation | PHP trait adaptation compile-time fatal error |
| `E_PHP_INVALID_CLASS_CONTEXT_NAME` | error | class-like validation | PHP invalid `self`/`parent`/`static` context fatal error |
| `E_PHP_INVALID_MAGIC_METHOD_SIGNATURE` | error | class-like validation | PHP magic method signature or staticness fatal error |
| `W_PHP_REFERENCE_BEHAVIOR_DEFERRED` | warning | any semantic layer | reference behavior exists but is intentionally deferred |
| `N_PHP_RUNTIME_CHECK_DEFERRED` | note | any semantic layer | runtime-only behavior intentionally deferred to later layers |

The mapping is intentionally coarse. Semantic frontend tracks stable local IDs and source
locations; exact PHP wording is not part of the compatibility contract.
