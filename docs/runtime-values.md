# Runtime Runtime Values

`php_runtime` contains the value model consumed by `php_vm`. It is intentionally
small enough for the Runtime executable subset and explicit about behavior that
is not yet PHP-compatible.

## Value Model

`Value` variants are:

- `Null`
- `Bool`
- `Int`
- `Float`
- `String`
- `Uninitialized`
- `Array`
- `Object`
- `Callable`
- `Reference`

`Uninitialized` is an internal VM state for locals and registers. Publicly
observable undefined-variable behavior is emitted as structured diagnostics and
covered by valid fixtures.

Fixture proof: `fixtures/runtime/valid/scalars/expressions.php`,
`fixtures/runtime/valid/variables/assignment.php`, and
`fixtures/runtime/valid/variables/undefined.php`.

## Strings

`PhpString` stores bytes. Runtime does not normalize Unicode, infer encodings,
or model mbstring/intl behavior. String constants and output bytes are compared
as bytes after the fixture harness performs only documented reference
normalization.

Fixture proof: `fixtures/runtime/valid/scalars/echo.php`,
`fixtures/runtime/valid/scalars/casts.php`, and
`fixtures/runtime/valid/builtins/var-dump-scalars.php`.

## Numbers and Conversions

Integers are `i64`. Floats preserve their `f64` bits through `FloatValue`.
Runtime semantics centralizes scalar conversion through `php_runtime::numeric_string` and
`php_runtime::convert`: truthiness, scalar casts, arithmetic, concat, selected
comparisons, and simple runtime type-family checks all use the shared
conversion API. Numeric strings are classified as int-string, float-string,
leading-numeric, or non-numeric in the committed fixture range.

Full PHP numeric-string compatibility is still a known gap. INF/NAN edge cases,
warning wording, overflow matrices, resources, extension-specific conversions,
and weak/strict coercion details are not treated as complete.

Fixture proof: `fixtures/runtime/valid/scalars/expressions.php`,
`fixtures/runtime/valid/scalars/comparisons.php`,
`fixtures/runtime/valid/scalars/casts.php`,
`fixtures/runtime/valid/runtime_types/param-int.php`, and
`fixtures/runtime/invalid/runtime_types/param-int-fail.php`.

## Arrays

`PhpArray` is an insertion-ordered map with `ArrayKey::Int` and
`ArrayKey::String`. Overwriting a key preserves its insertion position. Appends
use the next integer key. Iteration snapshots are used for by-value foreach.

The implemented key normalization supports:

- integers as integer keys;
- booleans as `0` or `1`;
- `null` as the empty-string key;
- finite floats truncated to integer keys;
- decimal integer strings without leading plus or leading zero as integer keys;
- all other supported strings as string keys.

Arrays, objects, callables, and references are not valid array keys in Runtime.
Array element references, full copy-on-write, array spread, Traversable sources,
and resource/object key edge cases remain known gaps.

Fixture proof: `fixtures/runtime/valid/arrays/indexed.php`,
`fixtures/runtime/valid/arrays/string-keys.php`,
`fixtures/runtime/valid/arrays/append-overwrite.php`,
`fixtures/runtime/valid/arrays/isset-empty-unset.php`,
`fixtures/runtime/valid/foreach/snapshot-mutation.php`, and
`fixtures/runtime/valid/references/array-element-ref.php`.

## Objects

`ObjectRef` is a reference-counted handle to object storage with a stable object
ID, class name, and public property map. Equality is by object ID. Clone creates
a new object identity with a shallow property-map copy; clone-with then applies
public property replacements.

The object MVP covers concrete classes, constructors, public properties, public
instance methods, simple public static methods, simple class-name type checks,
and public typed-property write checks. It does not implement visibility
scopes, inheritance/interface compatibility, readonly/asymmetric visibility,
property hooks, magic methods, `__clone`, dynamic properties, dynamic class
names, late static binding, or autoload-sensitive lookup.

Fixture proof: `fixtures/runtime/valid/objects/instantiate.php`,
`fixtures/runtime/valid/objects/constructor-property.php`,
`fixtures/runtime/valid/objects/property-read-write.php`,
`fixtures/runtime/valid/objects/clone-with.php`,
`fixtures/runtime/valid/objects/private-property.php`, and
`fixtures/runtime/known_gaps/property_hooks/get-hook.php`.

## References

`ReferenceCell` wraps shared mutable storage. `ValueSlot` is either a concrete
`Value` or a reference to a `ReferenceCell`; reads and writes dereference slots
so simple local aliases observe each other.

Simple local alias assignment, plain user-function by-reference parameters,
local/static-local by-reference returns, array-element references, and
by-reference closure captures are executable. By-reference foreach and complete
copy-on-write matrix coverage remain explicit known gaps.

Fixture proof: `fixtures/runtime/valid/references/local-alias.php`,
`fixtures/runtime/valid/references/by-ref-param.php`,
`fixtures/runtime/valid/references/by-ref-return.php`,
`fixtures/runtime/valid/functions/by-ref-capture.php`,
`fixtures/runtime/valid/references/array-element-ref.php`, and
`fixtures/runtime/known_gaps/foreach/by-ref.php`.

## Callables

`CallableValue` covers user functions, closures with by-value captures,
selected internal builtins, method placeholders, and unresolved dynamic
callables. The VM resolves simple user functions before builtins. The PHP 8.5
pipe MVP calls one resolved callable with the LHS value as its only argument.

Dynamic string functions including namespaced strings, array callables,
invokable objects, method callables, and first-class callable values execute
for covered fixtures. Imported/function-alias callable edges, closure binding,
and wider dynamic callable resolution remain known gaps.

Fixture proof: `fixtures/runtime/valid/functions/closure-use.php`,
`fixtures/runtime/valid/php85/pipe-user-function.php`,
`fixtures/runtime/valid/php85/pipe-builtin.php`,
`fixtures/runtime/invalid/php85/pipe-not-callable.php`, and
`fixtures/runtime/valid/functions/dynamic-call.php`.

## Diagnostics Boundary

Runtime values favor deterministic structured diagnostics over PHP CLI wording
compatibility. `RuntimeDiagnostic` IDs are the stable contract; reference text
matching is constrained to fixtures with explicit reference coverage.
Known deviations are tracked in `docs/runtime-known-gaps.md` and
`docs/runtime-reference-diff.md`.
