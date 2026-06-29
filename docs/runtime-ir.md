# Runtime IR

Runtime uses a small register-based IR as the executable contract between the
Semantic frontend semantic frontend and the Rust VM. The IR defines stable IDs, units,
functions, classes, basic blocks, instructions, terminators, constants,
operands, source spans, and source maps. It is executed by `php_vm`, but it
still deliberately stays below Zend ABI, extension, JIT, autoload, eval, and
complete runtime-compatibility layers.

## Versioning

`php_ir::IR_VERSION` is the schema version for serialized or snapshotted IR.
The initial version is `1`. Any incompatible change to the shape of `IrUnit`,
instruction payloads, terminators, ID meaning, or span semantics must increment
the version and update snapshot fixtures when those fixtures are introduced.

## IDs

IR references use typed newtype IDs instead of raw indexes:

- `UnitId`
- `FileId`
- `FunctionId`
- `ClassId`
- `BlockId`
- `InstrId`
- `LocalId`
- `RegId`
- `ConstId`

Each ID wraps a `u32` and exposes `new()` and `index()`. The wrapped value is an
index into the corresponding table for the containing IR object. Raw PHP token
numbers, parser node indexes, and semantic symbol internals are not exposed
through the IR ID surface.

## Unit Shape

`IrUnit` is the top-level container. It stores:

- `version`: current `IR_VERSION`.
- `id`: the unit ID.
- `files`: source file table entries used by spans.
- `constants`: unit constants.
- `functions`: function bodies.
- `function_table`: normalized user-function lookup entries.
- `constant_table`: runtime-visible global constant lookup entries.
- `classes`: class skeletons reserved for later lowering.
- `entry`: the entry function.
- `source_map`: source-map entries from IR targets back to Semantic frontend origins.

`IrFunction` stores its name, parameter metadata, locals, register count, basic
blocks, function source span, flags, return type, and closure captures. The
flags describe top-level, closure, and method placement. Type descriptors,
class entries, parameter defaults, and property descriptors are metadata
consumed by the VM; lowering still owns semantic decisions and unsupported
feature classification.

## Blocks and Control Flow

The IR is block-structured. A `BasicBlock` contains a linear instruction list
and an optional terminator. Control-flow exits are represented as terminators,
not ordinary instructions:

- `Jump`
- `JumpIfFalse`
- `JumpIfTrue`
- `JumpIf`
- `Return`

This keeps branch and return edges explicit at the block boundary and avoids
ambiguous instructions after control flow exits. `verify_unit()` requires every
block to end in a terminator.

## Final Instruction List

The Runtime instruction set is:

- `Nop`
- `LoadConst { dst, constant }`
- `FetchConst { dst, name }`
- `Move { dst, src }`
- `LoadLocal { dst, local }`
- `LoadLocalQuiet { dst, local }`
- `StoreLocal { local, src }`
- `BindReference { target, source }`
- `Binary { dst, op, lhs, rhs }`
- `Compare { dst, op, lhs, rhs }`
- `Unary { dst, op, src }`
- `Cast { dst, kind, src }`
- `Discard { src }`
- `Echo { src }`
- `CallFunction { dst, name, args }`
- `CallMethod { dst, object, method, args }`
- `CallStaticMethod { dst, class_name, method, args }`
- `CloneObject { dst, object }`
- `CloneWith { dst, object, replacements }`
- `EnterTry { catch, finally, after, exception_local }`
- `LeaveTry`
- `EndFinally { after }`
- `Throw { value }`
- `MakeException { dst, message }`
- `MakeClosure { dst, function, captures }`
- `CallClosure { dst, callee, args }`
- `ResolveCallable { dst, callable }`
- `CallCallable { dst, callee, args }`
- `Pipe { dst, input, callable }`
- `Include { dst, kind, path }`
- `NewObject { dst, class_name, args }`
- `FetchProperty { dst, object, property }`
- `AssignProperty { dst, object, property, value }`
- `NewArray { dst }`
- `ArrayInsert { array, key, value }`
- `FetchDim { dst, array, key, quiet }`
- `AssignDim { dst, local, dims, value }`
- `AppendDim { dst, local, dims, value }`
- `IssetLocal { dst, local }`
- `EmptyLocal { dst, local }`
- `UnsetLocal { local }`
- `IssetDim { dst, local, dims }`
- `EmptyDim { dst, local, dims }`
- `UnsetDim { local, dims }`
- `ForeachInit { iterator, source }`
- `ForeachNext { has_value, iterator, key, value }`
- `ForeachInitRef { iterator, local }`
- `ForeachNextRef { has_value, iterator, key, value_local }`
- `ArrayGet { dst, array, index }`
- `Unsupported { diagnostic_id }`
- `RuntimeError { diagnostic_id, message }`

The operator payload enums are deliberately small: unary plus/minus/not/bitnot,
binary arithmetic/concat/pow, comparisons for loose/strict equality and
ordering, casts to bool/int/float/string/array/object/void, and include kinds
for `include`, `include_once`, `require`, and `require_once`.

`Unsupported` carries a diagnostic ID so future lowering can preserve a known
gap without pretending to execute unsupported PHP behavior. `RuntimeError`
carries a stable diagnostic ID and message for deterministic runtime failures
that are not yet represented by full PHP exception classes.

## Instruction Families

Scalar expression instructions (`LoadConst`, `Binary`, `Compare`, `Unary`,
`Cast`, `Discard`, `Echo`) cover the green scalar fixtures in
`fixtures/runtime/valid/scalars/`.

Local variable instructions (`LoadLocal`, `LoadLocalQuiet`, `StoreLocal`,
`UnsetLocal`, `BindReference`) cover assignment, compound assignment, inc/dec,
simple unset, and simple local alias fixtures in
`fixtures/runtime/valid/variables/` and `fixtures/runtime/valid/references/`.

Call instructions (`CallFunction`, `CallClosure`, `ResolveCallable`,
`CallCallable`, `Pipe`, `CallMethod`, `CallStaticMethod`) cover direct user
functions, selected builtins, closures, arrow functions, first-class callable
names for the pipe MVP, public instance methods, and simple public static
methods. Dynamic calls, by-reference captures, and method/array/invokable
callables are explicit known gaps.

Array instructions (`NewArray`, `ArrayInsert`, `FetchDim`, `AssignDim`,
`AppendDim`, `IssetLocal`, `EmptyLocal`, `IssetDim`, `EmptyDim`, `UnsetDim`,
`ForeachInit`, `ForeachNext`, `ForeachInitRef`, `ForeachNextRef`, `ArrayGet`) cover literal arrays, scalar
dimension fetch/assign/append, query operations, unset, variadic packed access,
array-element reference binding, by-value foreach snapshots, and local-array
by-reference foreach binding. Temporary by-reference foreach sources remain a
known gap.

Object instructions (`NewObject`, `FetchProperty`, `AssignProperty`,
`CallMethod`, `CallStaticMethod`, `CloneObject`, `CloneWith`) cover concrete
classes, public instance properties, constructors, public methods, shallow
clone, and PHP 8.5 clone-with for public properties. Visibility, readonly,
property hooks, `__clone`, inheritance, interfaces, traits, enums, dynamic
properties, dynamic class names, and late static binding remain known gaps.

Exception instructions (`EnterTry`, `LeaveTry`, `Throw`, `EndFinally`,
`MakeException`) cover `throw new Exception("message")`, `try`, MVP
`catch (Exception|Throwable $e)`, `finally`, and deterministic uncaught
diagnostics. The full Throwable/Error hierarchy and stacktrace formatting are
not modeled.

`Include` covers root-constrained local include/require execution through the
same frontend-to-IR pipeline. It does not implement include_path, stream
wrappers, arbitrary filesystem policy, or cross-file symbol redeclaration
compatibility.

`Unsupported` carries a diagnostic ID so future lowering can preserve a known
gap without pretending to execute unsupported PHP behavior. `RuntimeError`
carries a stable diagnostic ID and message for deterministic runtime failures
that are not yet represented by full PHP exception classes.

## Operands and Constants

Instruction operands are registers, locals, or constants. Constants currently
cover null, booleans, integers, floats, and strings. This is a minimal value
description for IR construction and snapshots, not the runtime value model.

Constants are interned in deterministic first-use order by `IrBuilder`.
Lowering starts by interning the `Null` return constant for top-level code.
Subsequent literal constants reuse existing IDs when the IR constant payload is
identical.

Work item lowering recognizes Semantic frontend HIR literals for `null`, booleans,
integers, floats, and quoted strings, plus simple top-level `echo` expression
trees using unary `+`, `-`, `!`, `~`, arithmetic `+`, `-`, `*`, `/`, `%`, `**`,
concatenation `.`, comparisons, and scalar casts. It emits register-based
instructions and leaves evaluation to `php_vm`.

String literal lowering is byte-oriented. The lowering step strips the outer
quote delimiters for the IR constant payload and does not perform Unicode
normalization.

## Source Spans

Every instruction and terminator carries an `IrSpan`. Spans reference the IR
file table and preserve byte offsets as the source of truth. Line and column
display data remains derived outside this layer.

## Source Maps

`IrSourceMap` records stable mappings from IR functions, blocks,
instructions, and terminators back to Semantic frontend origin labels such as
`hir:module:0` and `hir:expr:1`. Each entry also carries an `IrSpan`.
Source-map entries are printed in text snapshots and serialized in JSON dumps,
so tests can verify that lowered bytecode remains traceable to the frontend
without re-parsing PHP.

## Example Dump

`php-vm dump-ir --with-source fixtures/runtime/valid/variables/assignment.php`
prints source lines followed by a deterministic IR snapshot:

```text
source path=fixtures/runtime/valid/variables/assignment.php
source 0001: <?php
source 0003: $a = 1;
source 0004: echo $a, "\n";
--- ir ---
ir version=1 unit=0 entry=function:0
files:
  file:0 ".../fixtures/runtime/valid/variables/assignment.php"
constants:
  const:0 null
  const:1 int 1
  const:2 string "\n"
functions:
  function "main" params=0 locals=1 regs=3 flags=top_level span=file:0@0..73
    local:0 $a
    block:0
      instr:0 span=file:0@55..56 load_const r0 const:1
      instr:1 span=file:0@50..56 store_local local:0 r0
      instr:2 span=file:0@50..56 discard r0
      instr:3 span=file:0@63..65 load_local r1 local:0
      instr:4 span=file:0@63..65 echo r1
      instr:5 span=file:0@67..71 load_const r2 const:2
      instr:6 span=file:0@67..71 echo r2
      term span=file:0@0..73 return const:0
```

The committed snapshot fixtures under `fixtures/bytecode/valid/` are the
reviewable regression corpus for this text format.

## Boundaries

This crate does not contain:

- a lexer, parser, or semantic frontend;
- eval, autoload, or complete PHP function lookup behavior.

Runtime values and dispatch live in `php_runtime` and `php_vm`; they consume
this IR contract but do not redefine it.

Work item adds `Include { dst, kind, path }`, where `kind` is one of
`include`, `include_once`, `require`, or `require_once`. The path operand is a
normal evaluated expression result. The VM resolves that path against the
currently executing file through a root-constrained local loader, compiles the
target through the same Frontend -> IR pipeline, and stores the include return
value in `dst`. Missing `include` stores `false` after emitting a warning
diagnostic; missing `require` fails with a fatal runtime diagnostic. `_once`
semantics are tracked by canonical path in the VM execution state.

## Lowering Skeleton

`php_ir::lower_frontend_result()` consumes a Semantic frontend `FrontendResult` and
produces a minimal top-level `IrFunction`. Work item lowers top-level scalar
`echo` statements into `LoadConst`, `Unary`, `Binary`, `Compare`, `Cast`,
`Discard`, and `Echo`, returns the interned `Null` constant from the synthetic
top-level function, and emits machine-readable unsupported-feature diagnostics
for HIR that the runtime layer cannot execute yet.

Work item adds deterministic local slots to `IrFunction.locals`. Variable names
are interned without the leading `$` in first-use order, and `local_count` must
match the local table length. Top-level scalar assignments and fetches lower to
`StoreLocal` and `LoadLocal`; compound assignments lower through the same
`Binary` operations used by scalar expressions; prefix and postfix inc/dec
lower through local load, scalar add/subtract, and store instructions.

Work item lowers executable top-level namespace statement items recursively
rather than iterating the whole HIR statement arena. `if`/`elseif`/`else`,
`while`, `do while`, simple `for`, `break`, and `continue` produce explicit
basic blocks with `Jump`, `JumpIfTrue`, and loop-stack targets. `JumpIfTrue`
uses the target as the truthy branch and the next physical block as false
fallthrough, which keeps nested control flow independent of enclosing block
allocation order. `for` lowering covers the simple initializer/condition/update
MVP; multiple expressions per header section remain a known gap.

Work item adds explicit `JumpIf` terminators for control-flow forms where both
branch targets must be deterministic independent of physical block order.
`switch` lowers to a chain of loose `Equal` comparisons against the subject,
case body blocks, an optional default fallback, and ordinary fallthrough from
one case body to the next unless a `break` terminates the block. `match` lowers
as an expression with strict `Identical` comparisons and result blocks that move
the selected arm value into a shared destination register. A no-arm `match`
emits `RuntimeError` with diagnostic ID `E_PHP_VM_UNHANDLED_MATCH`, which is
stable now and can later be mapped to PHP's `UnhandledMatchError`.

Short-circuit expressions lower to blocks that avoid RHS evaluation when PHP
would avoid it: `&&`/`and`, `||`/`or`, `??`, and ternary `?:`/`? :` write a
shared destination register only along the selected branch. Top-level `return`
terminates the current IR function with the returned value or `Null` when no
expression is present.

Work item lowers named function declarations into separate `IrFunction`s with
parameter locals and body blocks. `IrUnit.function_table` stores deterministic
normalized lookup names pointing at `FunctionId`s; dynamic calls are not folded
into this table. Direct named calls lower to `CallFunction { name, args, dst }`
after evaluating argument expressions left-to-right.

Work item records required/optional parameters, folded constant defaults,
variadics, by-reference scaffolding, and return-type descriptors for `int`,
`float`, `string`, `bool`, `null`, `void`, `mixed`, and class names. Class return
types are represented in IR but remain a VM known gap until object storage
exists. Omitted optional parameters use only folded Semantic frontend constant-expression
data. Too-few and too-many positional calls are stable VM runtime diagnostics;
named arguments and argument unpacking are still outside the MVP.

Work item lowers closure and arrow-function expressions into synthesized
`IrFunction`s with `FunctionFlags::is_closure`, explicit `IrCapture` metadata,
and a `MakeClosure` instruction at the expression site. Normal closures use
Semantic frontend explicit `use ($x)` metadata. Arrow functions derive by-value captures
from variables used in the arrow body, excluding parameters. Captured values are
assigned into the closure function's capture locals before parameter binding
when the VM enters the closure frame.

`use (&$x)` is detected and preserved as `by_ref=true` in the IR and snapshot
format. Runtime execution captures the source local's reference cell so later
mutations and writes through the closure observe the same storage. Full
`Closure::bind`, namespace/import fallback, and wider invalid-callable edge
cases remain outside this Work item MVP.

Work item supports first-class callable names in the actual Semantic frontend
HIR shape for pipe RHS (`HirExprKind::FirstClassCallable { callee: Name }`),
closure values stored in variables, dynamic string calls, array method
callables, static method callables, and invokable objects. Simple unqualified
names resolve in the VM to user functions first, then
`php_runtime::BuiltinRegistry` entries. Non-callable RHS values reach runtime
and fail with `E_PHP_VM_PIPE_RHS_NOT_CALLABLE`, preserving left-to-right
evaluation. Unresolved namespace/import fallback cases are represented as known
gaps, not silently executed.

Unsupported-feature diagnostic IDs are stable strings such as:

- `E_PHP_IR_UNSUPPORTED_GENERATOR`
- `E_PHP_IR_UNSUPPORTED_YIELD_FROM`
- `E_PHP_IR_UNSUPPORTED_FIBER`
- `E_PHP_IR_UNSUPPORTED_EVAL`
- `E_PHP_IR_UNSUPPORTED_REFLECTION`
- `E_PHP_IR_UNSUPPORTED_TRAIT_RUNTIME`
- `E_PHP_IR_UNSUPPORTED_ENUM_RUNTIME`
- `E_PHP_IR_UNSUPPORTED_PROPERTY_HOOKS`
- `E_PHP_IR_UNSUPPORTED_REFERENCE_SEMANTICS`
- `E_PHP_IR_UNSUPPORTED_HIR_STATEMENT`
- `E_PHP_IR_UNSUPPORTED_BY_REF_PARAMETER`
- `E_PHP_IR_UNSUPPORTED_BY_REF_RETURN`
- `E_PHP_IR_UNSUPPORTED_ADVANCED_PARAMETER`
- `E_PHP_IR_UNSUPPORTED_CLASSLIKE_OBJECT`
- `E_PHP_IR_UNSUPPORTED_OBJECT_METHOD_MODIFIER`
- `E_PHP_IR_UNSUPPORTED_OBJECT_PROPERTY_MODIFIER`
- `E_PHP_VM_UNRESOLVED_CALLABLE`
- `E_PHP_VM_PARAM_TYPE_MISMATCH`
- `E_PHP_VM_RETURN_TYPE_MISMATCH`
- `E_PHP_VM_PROPERTY_TYPE_MISMATCH`

Lowering diagnostics carry `IrSpan` values. The skeleton does not reimplement
Semantic frontend semantic checks and does not execute PHP.

## Snapshot Format

`IrUnit::to_snapshot_text()` is the stable text format used by
`fixtures/bytecode/valid/*.ir.snap`. It prints tables in vector order and avoids
hash-map iteration. The format is intended for review and regression tests, not
as a long-term interchange format.

`IrUnit::to_json_pretty()` exposes serde JSON for tools that need structured IR
without parsing the text snapshot format.

## Verifier Invariants

`php_ir::verify_unit()` checks the Work item structural invariants:

- supported `IR_VERSION`;
- valid entry function;
- file and class table IDs matching their table positions;
- spans pointing at known files with non-decreasing byte ranges;
- block and instruction IDs matching their table positions;
- valid destination and operand registers;
- valid locals and constants;
- valid closure target function IDs and capture operands;
- valid jump targets;
- valid exception/finally edge targets;
- valid call argument operands and by-reference metadata;
- register operands defined on every reachable incoming control-flow path before
  use;
- every block has a terminator.

Performance extends the same verifier for optimizer/cache/JIT preparation. See
`docs/performance-ir-verifier.md` for the Performance pre/post optimizer boundary,
optimizer-sensitive instruction families, and stable verifier diagnostic IDs.
