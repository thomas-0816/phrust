# Function Signatures

Function signature lowering records PHP function-like declarations without
executing calls, evaluating defaults, or introducing a runtime type system.

The lowered records live in `HirModule::signatures()` and are emitted in JSON
under `module.signatures`. Parameter and return types link to `TypeId` records
from the Semantic frontend type-lowering pass.

## Signature Forms

The signature HIR covers:

- named functions
- class, trait, interface, and enum methods
- closures
- arrow functions

Each `FunctionSignature` records its kind, optional source name, source span,
by-reference return marker, optional return type, ordered parameters,
function-like flags, and arrow-function body span when applicable.

The semantic frontend provides `FunctionLikeFlags` for:

- by-reference returns
- static methods, static closures, and static arrows
- generator detection through direct `yield` and `yield from`
- `void` and `never` return-type markers
- deferred generator-rule metadata
- conservative `$this` availability

## Parameters

Each parameter records:

- variable name, including the `$` prefix
- optional lowered `TypeId`
- by-reference marker
- variadic marker
- default-value source reference
- attribute-group source spans
- constructor-promotion metadata when modifiers are present

Default values are only source references in Semantic frontend. They are marked as
constant-expression candidates for signature contexts, but this pass does not
evaluate them or lower function calls.

## Constructor Property Promotion

Promoted constructor parameters record:

- base visibility: `public`, `protected`, or `private`
- `readonly`
- optional asymmetric set visibility
- modifier span

Promotion is only accepted in constructor context. The lowering pass reports a
stable diagnostic when promotion appears outside a constructor, on an abstract
constructor, or inside an interface.

## Validation

Signature lowering emits stable diagnostics for:

- duplicate parameter names within one signature
- variadic parameters that are not final
- default values on variadic parameters
- invalid constructor property promotion contexts
- closure `use` variables that duplicate parameter names
- duplicate closure `use` variables
- value returns from `void` functions
- explicit returns from `never` functions
- invalid parameter and return type contexts delegated to type lowering

The pass intentionally does not perform overload resolution, call validation,
runtime type checks, generator object behavior, closure capture-value
evaluation, `$this` runtime availability checks, or default-value evaluation.

## Fixture Coverage

`fixtures/semantic/functions/` covers basic signatures, by-reference and
variadic parameters, defaults, duplicate parameters, variadic ordering, and
closure-use duplicate validation. The semantic frontend also covers void/never returns,
generator flags, closure captures, arrow function bodies, and static closure
`$this` diagnostics.

`fixtures/semantic/classes/constructor-promotion.php` covers valid promoted
properties, including `readonly` and asymmetric set visibility.

`fixtures/semantic/classes/promotion-invalid.php` covers invalid promotion
contexts. These fixtures are compared with the pinned PHP 8.5.7 lint oracle
when a reference binary is available.
