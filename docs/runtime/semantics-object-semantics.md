# Runtime semantics Object Semantics

This document tracks the Runtime semantics object-model contract as it grows from the
Runtime object MVP. The runtime still consumes the single frontend pipeline:
`php_lexer` -> `php_syntax` -> `php_ast` -> `php_semantics` -> `php_ir` ->
`php_runtime` -> `php_vm`.

## Scope

The runtime provides the first executable class hierarchy layer:

- IR `ClassEntry` stores the normalized parent class name from HIR `extends`.
- Runtime `ClassEntry` carries parent metadata for object construction and
  diagnostics.
- VM object construction finalizes inherited instance properties from parent to
  child order before creating an `ObjectRef`.
- VM method dispatch walks the IR class hierarchy and preserves the declaring
  class for visibility checks.
- `$obj->method()`, `Class::method()`, `self::method()` and `parent::method()`
  use the same hierarchy lookup helper.

The runtime extends call frames with explicit `scope_class`, `called_class`, and
`declaring_class` metadata. `static::` resolves from the called class, while
`self::` and `parent::` use class scope. Explicit `Class::method()` calls are
non-forwarding; `self::`, `parent::`, and `static::` preserve the called class
for fixture-covered late static binding.

## Method Lookup

Instance and static method lookup starts at the runtime target class and walks
parents until a matching normalized method name is found. The resolved method
keeps both:

- the declaring class, used for visibility diagnostics and function dispatch;
- the method table entry, used for static/non-static checks and the function ID.

Private methods are scope-sensitive. If a method body declared on a parent class
calls `$this->x()` on a child object and the child also declares private `x`,
lookup skips the child private method for that parent scope and can resolve the
parent private method.

Missing parent classes and inheritance cycles produce deterministic VM errors:

- `E_PHP_VM_UNKNOWN_PARENT_CLASS`
- `E_PHP_VM_CLASS_INHERITANCE_CYCLE`

## Visibility

Implemented baseline visibility checks:

- `public`: callable from any scope.
- `private`: callable only from the declaring class scope.
- `protected`: callable from the declaring class or a subclass scope.

External calls to inaccessible methods produce:

- `E_PHP_VM_PRIVATE_METHOD_ACCESS`
- `E_PHP_VM_PROTECTED_METHOD_ACCESS`

Abstract methods remain outside the concrete VM dispatch MVP and produce
`E_PHP_VM_ABSTRACT_METHOD_CALL` if reached.

## Properties

The runtime finalizes inherited instance property slots at object construction so
parent-declared properties are present on child instances. The runtime extends
that model with declared defaults, typed uninitialized slots, public/protected/
private read-write checks, initialize-once readonly property and readonly-class
writes, per-class static property storage, dynamic-property creation
deprecations, and instance-property `isset`, `empty`, and `unset`.

Private instance properties use declaring-class storage keys so a parent and
child may both declare `$x` without collapsing into a single slot. Public and
protected instance properties keep their source property name as the storage
key. Static properties live in VM execution state keyed by normalized declaring
class and property name, not on object instances.

## Late Static Binding and Class Metadata

The runtime provides executable late static binding for static and instance method
frames. Method call frames preserve:

- `scope_class` for `self::`, `parent::`, visibility, and private storage;
- `called_class` for `static::` and `static::class`;
- `declaring_class` for the method body selected by hierarchy lookup.

Class constants are lowered into `ClassEntry.constants` separately from
properties. Runtime constant fetch walks the class hierarchy, enforces public,
protected, and private visibility, and supports simple folded/literal
constant-expression values. `Foo::class`, `self::class`, `parent::class`, and
`static::class` return source-spelled class names while lookup still uses
normalized names internally.

The covered private LSB edge follows PHP: `static::privateMethod()` resolves
against the called class and can fail when the resolved private method is not
visible from the declaring scope.

## Interfaces, Abstract, and Final

The runtime provides runtime-visible interface metadata to IR and VM class entries.
Interface declarations are preserved in the class table, class `implements`
clauses and interface `extends` clauses are normalized into `interfaces`, and
the VM registers minimal internal interface metadata for `Iterator`,
`IteratorAggregate`, `Throwable`, `UnitEnum`, `BackedEnum`, and `Stringable`.

Before entry execution, the VM validates:

- missing or non-interface `implements` / interface `extends` targets;
- final class extension;
- final method overrides;
- concrete classes that still expose inherited abstract methods;
- fixture-scoped interface method implementation, public visibility, and
  parameter/return metadata compatibility. Implementations may make interface
  parameters optional and add optional trailing parameters in the covered MVP.

`instanceof` uses the same class/interface hierarchy helper as declared
class-like parameter, return, and property type checks. The runtime extends the
internal Throwable metadata layer so VM-created `Exception`, `Error`,
`TypeError`, `ValueError`, `ArgumentCountError`, and `FiberError` objects
participate in `Throwable`/`Error`/exact-class checks and typed `catch`
matching.

Full PHP variance, complete SPL method surfaces, userland `Throwable`
implementation rules, and exact engine diagnostic text remain later work.

## Exceptions and Runtime Errors

The runtime keeps exceptions as VM metadata objects instead of full userland
class instances, but the visible hierarchy is now stable for the covered
runtime subset:

- `Exception` implements `Throwable`;
- `Error` implements `Throwable`;
- `TypeError`, `ValueError`, `ArgumentCountError`, and `FiberError` extend
  `Error`;
- uncaught throwables report `E_PHP_VM_UNCAUGHT_EXCEPTION` with the visible
  throwable class name and message;
- recoverable runtime warnings continue execution and remain structured
  diagnostics instead of PHP-formatted stderr text.

`try`/`catch`/`finally` preserves visible `finally` ordering for normal
completion, returns, thrown try bodies, and nonmatching catches that are then
caught by an outer handler. The remaining error-model gaps are exact PHP warning
wording/channel parity, full stack trace/code/previous exception fields,
multi-catch/body lowering beyond the current lowered catch body, and
destructor/generator/fiber interactions with pending exception control flow.

## Enums

The runtime provides executable enum metadata and runtime singleton cases for the
fixture-covered language subset:

- enum declarations are lowered into the class table with `is_enum` metadata;
- unit enums implicitly implement `UnitEnum`, and backed enums also implement
  `BackedEnum`;
- enum cases are stored in source order and fetched through the existing
  `Enum::Case` class-constant path;
- each case is a per-execution singleton `ObjectRef`, so repeated case fetches
  and values returned from `cases()`, `from()`, and user methods compare
  identically with `===`;
- case objects expose readonly `name` and, for backed enums, readonly `value`
  properties through the ordinary property-fetch path;
- backed `from()` and `tryFrom()` use strict int/string backing-value lookup;
- enum methods, static methods, constants, and fixture-covered trait-composed
  methods reuse the normal class method/constant/trait lowering paths;
- direct `new EnumName()` fails with `E_PHP_VM_ENUM_INSTANTIATION`.

The runtime exposes enum-case attribute metadata through the Reflection metadata
MVP described below. Full `ReflectionEnum` public APIs remain outside this
subset. Serialization behavior is also an explicit
`E_PHP_RUNTIME_UNSUPPORTED_ENUM_SERIALIZATION` gap rather than an
approximation.

## Attributes and Reflection Metadata

The runtime carries Semantic frontend attribute metadata into IR and runtime class tables
for classes, functions, methods, parameters, properties, class constants, and
enum cases. Each runtime attribute entry preserves:

- source-spelled name for `ReflectionAttribute::getName()`;
- resolved and fallback names for later class-table/constructor semantics;
- source span metadata;
- argument values evaluated through the same folded constant-expression path
  used by runtime constants;
- a repeated-on-target marker for future validation.

The runtime broadens the VM Reflection metadata MVP for framework-style smoke
queries, and the runtime provides enum, closure, and structured callable reflection:

- `ReflectionClass::__construct`, `getName`, `getAttributes`, `getMethod`,
  `getMethods`, `getProperty`, `getProperties`, `getConstant`,
  `getConstants`, `getReflectionConstant`, `getReflectionConstants`,
  `getInterfaceNames`, `isInterface`, `isTrait`, `isEnum`, `isAbstract`,
  `isFinal`, `isInstantiable`, `getFileName`, `getStartLine`, `getEndLine`,
  and `getDocComment`;
- `ReflectionFunction::__construct`, `getName`, `getAttributes`,
  `getParameters`, `getNumberOfParameters`,
  `getNumberOfRequiredParameters`, `getReturnType`, source-location methods,
  and `getDocComment`;
- `ReflectionMethod::__construct`, function-style metadata plus
  `getDeclaringClass`, `isPublic`, `isPrivate`, `isProtected`, `isStatic`,
  `isAbstract`, and `isFinal`;
- `ReflectionProperty`, `ReflectionClassConstant`, `ReflectionParameter`, and
  `ReflectionEnumUnitCase` names, attributes, declaring class where
  applicable, visibility/static/readonly flags, types, defaults, values, and
  backed enum case values;
- `ReflectionEnum::__construct`, `getName`, `getAttributes`, `isBacked`,
  `getBackingType`, `getCases`, source-location methods, and `getDocComment`;
- `ReflectionNamedType::getName`, `allowsNull`, `isBuiltin`, and `__toString`;
- `ReflectionFunction` construction from closures and first-class user-function
  callables, with closure `isClosure`, `getStaticVariables`,
  `getClosureScopeClass`, parameters, and return type;
- `ReflectionAttribute::getName` and `getArguments`.

Reflection uses the runtime class table as the source of truth. Normalized
lookup names stay internal; displayed class and interface names use source
spelling where the class table preserves it. Source file/start/end line
metadata is best-effort from IR spans. Doc comments currently return `false`
because comment retention is not yet wired into IR metadata.
Callables are reflected from structured runtime `CallableValue` metadata. The
The MVP supports closures and user-function callables; method, internal
builtin, and unresolved dynamic callable reflection report
`E_PHP_VM_REFLECTION_UNSUPPORTED_CALLABLE` instead of stringifying unstable
callable placeholders.

Reflection objects are VM metadata handles, not userland class instances.
`ReflectionAttribute::newInstance()` is implemented for a bounded userland
attribute slice: metadata-backed class names are resolved from the request class
table, positional folded arguments are passed to the attribute constructor, and
constructor failures use normal VM call semantics. Attribute target and
repeatability validation, autoload-sensitive lookup, named argument parity,
internal attributes, and exact diagnostic text remain explicit
`E_PHP_RUNTIME_UNSUPPORTED_ATTRIBUTE_NEWINSTANCE` gaps.

## Property Magic

The runtime provides property overloading for instance property operations:

- missing or inaccessible property reads dispatch public non-static
  `__get(string $name)` before falling back to undefined-property warnings or
  visibility errors;
- missing or inaccessible writes dispatch public non-static
  `__set(string $name, mixed $value)` before dynamic-property creation or
  visibility errors;
- `isset($obj->x)` dispatches `__isset(string $name)` only when normal
  property-state lookup does not resolve an accessible declared/dynamic slot;
- `empty($obj->x)` uses `__isset` first and only dispatches `__get` when
  `__isset` returns truthy;
- `unset($obj->x)` dispatches `__unset(string $name)` for missing or
  inaccessible properties before normal unset fallback behavior.

Accessible declared properties keep the ordinary declared-property path.
Property magic applies to the inaccessible or missing cases covered by PHP's
property-overloading rules. `__set` on a missing property intercepts before
dynamic-property creation; if no usable `__set` exists, the existing dynamic
property path and deprecation diagnostic still apply.

The VM tracks active `(object, magic method, property)` dispatches and raises
`E_PHP_VM_MAGIC_PROPERTY_RECURSION` instead of recursing until a Rust stack
overflow. By-reference property lvalues through `__get` remain an explicit
reference-model gap covered by
`E_PHP_IR_UNSUPPORTED_PROPERTY_REFERENCE`.

The runtime provides executable PHP 8.4+ property hooks for the fixture-covered
runtime paths. The parser emits `PROPERTY_HOOK_DECL` nodes, Semantic frontend HIR records
hook kind/body/span metadata, and IR lowering creates synthetic method-like hook
functions with source-map entries such as
`hir:property-hook:Class::$prop:get`. The VM invokes those functions through the
same call-frame mechanism used by ordinary methods, with `$this` and class scope
set to the receiver.

Accessible declared-property reads dispatch `get` hooks before reading backing
storage. Accessible declared-property writes dispatch `set` hooks before normal
storage mutation. Missing or inaccessible properties still follow the
property-magic path, so `__get`/`__set` remain the fallback for PHP property
overloading cases rather than overriding accessible declared hooks.

Hooked properties can be backed or virtual in the runtime metadata. Virtual
hooked properties do not allocate an object storage slot; a write without a
usable `set` hook raises `E_PHP_VM_VIRTUAL_PROPERTY_WRITE`. Backed hooks use
the existing declared-property slot. While a hook is active for the same
`(object, class, property)`, direct `$this->prop` access bypasses hook dispatch
and reaches backing storage, which prevents accidental self-recursion and lets
fixture-covered backed hooks read or update their slot. Cross-property hook
recursion is guarded with `E_PHP_VM_PROPERTY_HOOK_RECURSION`.

The implemented asymmetric visibility subset carries `private(set)` and
`protected(set)` flags into IR/runtime metadata and applies them to normal
writes and clone-with replacements. Fixture-covered public hook writes still
run through the same property type and readonly checks as ordinary declared
property writes.

Remaining property-hook gaps are intentionally explicit:

- full PHP hook grammar and inheritance/override compatibility;
- explicit hook parameter syntax beyond the MVP implicit `set` value parameter;
- exact engine diagnostic wording for visibility and recursion failures;
- readonly/property-hook incompatibility matrices beyond existing runtime
  checks;
- by-reference hook/lvalue semantics, still blocked by
  `E_PHP_IR_UNSUPPORTED_PROPERTY_REFERENCE`;
- property defaults immediately before hook lists in the current parser.

The object-model architecture decision is recorded in
`docs/runtime/semantics-object-semantics.md`.

## Method and Object Magic

The runtime provides method and object magic for the fixture-covered runtime paths:

- missing or inaccessible instance method calls dispatch public non-static
  `__call(string $name, array $args)`;
- missing or inaccessible static method calls dispatch public static
  `__callStatic(string $name, array $args)`;
- object callables dispatch `__invoke` through the existing callable path, so
  invokable objects can be used by dynamic callable calls and the pipe operator;
- echo, concatenation, and explicit string casts use the VM string-conversion
  path and dispatch public non-static `__toString()`;
- exceptions thrown from `__toString` propagate through the VM exception path
  instead of being swallowed or converted into placeholder text.

The `__call` and `__callStatic` argument arrays preserve positional arguments
as packed array elements and named arguments as string-keyed elements. Active
method-magic dispatch is guarded by `(receiver, magic method, called method)`
and raises `E_PHP_VM_MAGIC_METHOD_RECURSION` on re-entry.

`__debugInfo` is executed for `var_dump` when an object declares a public
instance method. The returned array is formatted as debug properties on the
original object handle, preserving string and integer property labels for the
fixture-covered path. Wider recursion and exact diagnostic parity remain tracked
under the broader magic-method gap.

## Clone and Clone-With

The runtime completes the fixture-covered clone path:

- `clone $object` creates a new object identity with a shallow copy of the
  runtime property map;
- public non-static `__clone()` is invoked after the base copy is created and
  receives the clone as `$this`;
- exceptions thrown from `__clone` propagate through the VM exception path;
- `clone($object, [...])` creates the clone, runs `__clone`, then applies the
  replacement property writes to the clone only;
- replacement writes keep using declared-property type checks and do not mutate
  the original object.

The shallow copy preserves runtime `Value` payloads, including `ReferenceCell`
values if an object property already contains one. Direct declared and dynamic
object-property storage references now participate in the reference-cell model,
so clone reference preservation is covered by
`fixtures/runtime_semantics/clone_with/reference-property.php`.
Property-hook and magic-property reference lvalues plus invalid typed-property
writes through reference cells remain tracked under
`E_PHP_IR_UNSUPPORTED_PROPERTY_REFERENCE`.

Clone-with replacement of private, protected, readonly, static, or wider
hook-backed properties remains intentionally specific rather than approximated:

- private/protected replacement attempts outside the asymmetric setter
  subset route through the regular property visibility `Error` path;
- readonly replacement attempts route through the readonly write `Error` path;
- static replacement attempts still produce
  `E_PHP_VM_UNSUPPORTED_PROPERTY_MODIFIER`;
- fixture-covered public hooked replacements dispatch `set` hooks on the clone
  after `__clone`;
- wider hook replacement cases remain documented gaps until the full PHP
  visibility, readonly, inheritance, and reference matrix is implemented.

## Destructors and Lifetime

The runtime provides a VM-owned shutdown `DestructorQueue` rather than running
PHP code from Rust `Drop`. Objects with a public non-static `__destruct()` are
registered after successful construction and after successful clone creation.
At successful request shutdown, the queue drains in reverse registration order.
If a destructor creates another destructible object, that object is appended to
a later drain batch. A destructor exception or runtime error stops shutdown and
returns the destructor runtime error with previously written output preserved.

The queue executes each queued object identity once per queue residency and has
a 4096-execution overflow guard. Exact refcount-triggered destruction,
`unset()`-time destruction, cyclic-object collection, destructor ordering for
wider global/local shutdown cases, and generator/fiber interactions remain
explicit gaps. The queue invariants are recorded in
`docs/runtime/semantics-contract.md`.

## Known Boundaries

The object model still deliberately does not complete:

- full interface variance and SPL/internal interface method surfaces;
- trait properties/constants/nested trait uses and exact trait diagnostics;
- by-reference property lvalues;
- `__debugInfo` execution and serialization magic execution
  (`E_PHP_RUNTIME_SERIALIZATION_STDLIB_GAP`);
- clone-with private/protected/readonly/static/property-hook interactions.

Those items remain tracked in `docs/runtime/semantics-known-gaps.md` and later Runtime semantics
work items.

## Public API Surface

Standard library should reuse these runtime metadata APIs:

- `php_runtime::ClassEntry` and class member entries for methods, properties,
  constants, enum cases, attributes, hooks, and flags;
- `php_runtime::ObjectRef` for object identity and property storage;
- `php_runtime::RuntimeType` and type helper functions for class/interface
  checks and property/parameter/return validation;
- VM class tables, method dispatch, property lookup, destructor queue, and
  Reflection metadata handles.

Object performance hot spots are hierarchy lookup, visibility checks, property
hook dispatch, magic-method recursion guards, Reflection metadata construction,
and repeated enum-case/static-property access. Standard library may cache those paths,
but cache invalidation must account for include/eval/autoload additions to the
request class table.
