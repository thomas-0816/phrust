# Runtime Runtime Contract

Runtime turns the Semantic frontend semantic frontend into an executable MVP:

```text
SourceText
  -> php_lexer
  -> php_syntax
  -> php_ast
  -> php_semantics / HIR
  -> php_ir
  -> php_vm
  -> php_runtime
```

The target remains PHP `8.5.7`, git tag `php-8.5.7`, from
`https://github.com/php/php-src.git`.

## Scope

Runtime introduces a versioned, source-mapped IR, a deterministic interpreter
VM, a minimal runtime value model, and a CLI that can compile, dump, execute,
and compare small PHP programs. The only regular input to Runtime is Semantic
frontend output, preferably through
`php_semantics::query::frontend::analyze_file`.

Runtime may execute only files that pass parser and semantic frontend checks.
Parser diagnostics and semantic diagnostics remain pre-bytecode gates. Runtime
diagnostics are separate diagnostics produced by the IR, VM, or runtime layer.

## Non-Goals

- No second lexer, parser, CST, AST, HIR, or semantic frontend.
- No full PHP standard library.
- No Zend extension ABI emulation.
- No FPM, FastCGI, web SAPI, or server runtime.
- No Opcache, quickening, inline cache system, or JIT.
- No full Copy-on-Write, refcount heap, cycle collector, or complete PHP
  reference semantics.
- No complete resources, streams, wrappers, SPL, Reflection, DateTime, JSON,
  PCRE, mbstring, intl, PDO, or extension surface.
- No vendored `php-src` checkout or generated reports under `target/`.

## MVP Support

The Runtime MVP should execute:

| Area | MVP support | Initial status |
| --- | --- | --- |
| CLI | compile, dump IR, run, compare | compile, dump-ir, run implemented; compare reserved |
| VM | entry dispatch, frames, registers, return, echo | scalar expression dispatch implemented |
| Values | null, bool, int, float, string, arrays, simple objects | scalar model, ordered PHP array MVP, and `ObjectRef` MVP implemented |
| Output | stdout capture, print/echo behavior | byte buffer, scalar echo, and `print` return value implemented |
| Exit codes | success, compile error, runtime error, unsupported, internal error, usage error | status classifications implemented |
| Variables | locals, script-scope variables, assignment, unset, simple local aliases | top-level locals, assignments, compound assignments, scalar inc/dec, local unset, and `$b =& $a` MVP implemented |
| Expressions | arithmetic, comparisons, casts, truthiness, concatenation | scalar MVP implemented |
| Control flow | if/elseif/else, loops, break, continue, switch, match, return | implemented for the Runtime scalar subset |
| Functions | user functions, parameters, defaults, variadics, recursion | named declarations, positional by-value params, folded scalar defaults, packed-list variadics, scalar/nullable parameter checks, scalar/nullable/void return checks, frames, returns, recursion implemented |
| Closures | by-value/by-reference capture and arrow functions | closure values, `use ($x)`, `use (&$x)`, arrow by-value capture, closure returns, static closure locals, and closure calls implemented; full `Closure::bind` compatibility remains a known gap |
| Arrays | ordered int/string-key map, literals, fetch, assign, append, foreach | literals, local dim operations, array-element references, by-value array foreach, and local-array by-reference foreach implemented; by-value foreach snapshots insertion-ordered entries at loop entry; temporary by-reference sources and Traversable objects are known gaps |
| Constants | user constants and MVP magic constants | global `const` declarations with folded Semantic frontend const-expression values, `FetchConst`, `PHP_VERSION`, undefined-constant runtime errors, and top-level/function/method magic constants implemented; `define()` and the full predefined constants matrix remain gaps |
| Includes | local include/require loader without stream-wrapper compatibility | root-constrained local file loader implemented for `include`, `include_once`, `require`, and `require_once` |
| Runtime context | deterministic CLI argv/env context and controlled superglobals | `RuntimeContext` carries `cwd`, `argv`, controlled `env`, `include_path`, ini-like placeholders, error_reporting placeholder, and strict_types metadata placeholders; `$argc`, `$argv`, `$_SERVER['argc']`, `$_SERVER['argv']`, explicit sorted `$_ENV`, empty request-style arrays, and placeholder `GLOBALS` are seeded for top-level fixtures |
| Objects | class table, `new`, public properties, constructor `$this`, public instance methods, simple public static methods, shallow clone | implemented for Work item object/method/clone/typecheck MVP; simple public typed-property writes are checked at runtime; Runtime semantics adds visibility dispatch, late static binding, static properties, readonly basics, magic methods, clone magic, public clone-with replacements, and fixture-covered property hooks; private/protected/readonly/static/full-hook clone-with matrices remain known gaps |
| Exceptions | throw, try, catch, finally, uncaught exception exit behavior | Work item implements VM-internal `Exception` objects, `throw`, `catch (Exception $e)`/`Throwable`, uncaught exception diagnostics, rethrow, and `finally` on normal return or thrown control-flow; full Throwable hierarchy, typed catch matching beyond the MVP, stacktrace wording, and catch-throw-through-finally compatibility remain known gaps |
| PHP 8.5 | pipe operator, `(void)` discard, clone-with MVP | pipe MVP implemented; clone-with executes for public untyped object properties; `(void)` IR discard modeled while executable `(void)` programs remain a frontend/runtime gap |

## Known-Gap Policy

Unsupported features must be explicit. A gap is acceptable only when it is:

| Field | Requirement |
| --- | --- |
| `id` | Stable diagnostic ID such as `E_PHP_IR_UNSUPPORTED_GENERATOR` |
| `feature` | Human-readable feature name |
| `status` | `planned`, `known_gap`, `implemented`, or `deferred` |
| `fixture` | Fixture path or `planned` until the fixture work item creates it |
| `reference_behavior` | PHP reference behavior or `documented-later` |
| `rust_behavior` | Runtime diagnostic, skip classification, or implemented behavior |
| `resolution_phase` | `runtime`, `runtime-semantics`, or a narrower named layer |

The runtime may reject unsupported runtime features with a stable diagnostic and
unsupported exit classification. It must not silently execute an unsupported
feature with wrong semantics.

## Reference Model

The primary runtime oracle is the pinned PHP CLI at
`third_party/php-src/sapi/cli/php`, or `REFERENCE_PHP` when explicitly set.
Reference-dependent checks must skip clearly when no usable reference binary is
available. If `REFERENCE_PHP` is set but unusable, the check must fail.

Runtime comparisons normalize volatile output details and compare:

- exit classification
- stdout
- normalized stderr or diagnostic stream
- stable runtime diagnostic IDs
- optional IR, VM, or trace snapshots

Raw PHP numeric token IDs, exact host paths, and unstable wording should not be
the compatibility contract.

## Runtime Context

`php_runtime::RuntimeContext` is an owned, reproducible configuration object.
It is separate from the mutable builtin execution context. The Runtime VM uses
it only to seed recognized main top-level locals by name: `$argc`, `$argv`,
`$_SERVER`, `$_ENV`, `$_GET`, `$_POST`, `$_COOKIE`, `$_FILES`, `$_REQUEST`,
and `GLOBALS`.

The default context imports no process environment. CLI execution constructs a
controlled context where `argv[0]` is the script path string passed to
`php-vm run`, and script arguments are accepted only after `--`. `$_ENV` is
populated only from explicit context entries sorted by key/value; host
environment variables do not leak into runtime fixture output. Request-style
superglobals are empty arrays until a future SAPI/request layer exists.
`GLOBALS` is an empty placeholder and does not implement PHP's live global
symbol-table aliasing semantics.

The current `ini`, `error_reporting`, `include_path`, `cwd`, and `strict_types`
fields are contract placeholders for later runtime behavior. They are carried
as deterministic data but are not a full INI parser, PHP error-reporting
engine, include_path search implementation, or strict/weak typing policy.

## Exit-Code Model

Rust CLI numeric values may evolve until the CLI crate exists, but the
classifications are stable:

| Classification | Meaning |
| --- | --- |
| `success` | Program executed successfully |
| `runtime_error_or_uncaught_exception` | Runtime fatal error or uncaught exception |
| `frontend_or_compile_error` | Parser, semantic frontend, IR lowering, or compile error |
| `unsupported_feature_known_gap` | Explicit unsupported feature or known gap |
| `internal_error_or_ir_verifier_failure` | Internal invariant or verifier failure |
| `cli_usage_error` | Invalid CLI usage |

## Runtime Diagnostics MVP

Work item introduces `php_runtime::RuntimeDiagnostic` as the stable diagnostic
payload shared by the VM and CLI. Each diagnostic has a stable ID, severity,
message, source span, deterministic stack-frame list, and optional
PHP-reference classification. `RuntimeSeverity` covers warnings, notices,
deprecations, recoverable runtime errors, fatal errors, and unsupported
features.

`RuntimeError` wraps diagnostic errors separately from `php_vm::VmControlFlow`,
so normal error reporting is not represented as `return`, `break`, `continue`,
or the future `throw` control signal. Current helper constructors cover
undefined variable warnings, TypeError MVP errors, division-by-zero MVP errors,
undefined functions, undefined constants, and unsupported features.

The CLI prints runtime diagnostics as compact JSON on stderr with a
`runtime-diagnostic:` prefix before the legacy status line. Runtime diagnostic
JSON omits host-specific paths unless a runtime source span is available; stack
frames are function names ordered from current frame back to `main`, making
snapshot output deterministic enough for fixtures.

## Runtime Value Model

`php_runtime::Value` currently supports `Null`, `Bool`, `Int`, `Float`,
`String`, and `Uninitialized`. `Array` stores an opaque `PhpArray` ordered-map
facade with normalized `ArrayKey::Int` or `ArrayKey::String` keys. The current
storage is a simple insertion-ordered vector, but the public API is shaped so a
future packed/mixed representation can replace it without exposing storage or
blocking Copy-on-Write work. `Callable` stores a `CallableValue` abstraction for
user functions, closures, selected
internal builtins, method placeholders, and unresolved dynamic gaps. Closure
values carry a runtime-local raw target function ID plus captured names and
cloned values so `php_runtime` does not depend on `php_ir`. `ReferenceCell` and
`ValueSlot` provide the Work item local-alias scaffold without exposing
`Rc<RefCell<Value>>` through public VM APIs. `Object` and explicit
`Value::Reference` values remain placeholders for later layout, refcount, and
copy-on-write work. Object method calls execute by looking up public method
metadata in the lowered class table. `$this` is bound only for instance method
frames; top-level code and static method frames report
`E_PHP_VM_THIS_OUTSIDE_METHOD` if they read `$this`.

`PhpArray` supports insert, get, get_mut, remove, append, len, and deterministic
insertion-order iteration. Key conversion covers the tested MVP range:
int keys remain integers; bools become `0`/`1`; null becomes the empty string;
floats truncate toward zero; decimal integer strings without a leading plus and
without leading zeroes become integer keys; other strings remain string keys.
Full PHP edge cases around overflow, NAN/INF, locale, and numeric-string
classification remain known gaps.

`php_runtime::PhpString` stores bytes and does not impose a UTF-8 invariant.
Display and debug helpers are developer-facing conveniences only; they are not
PHP-compatible `var_dump` output.

`php_runtime::OutputBuffer` captures exact bytes and exposes lossy text only for
tests and diagnostics. `ExecutionStatus`/`ExitStatus` provide stable status
classes for success, compile error, runtime error, unsupported features, and
fatal failures.

## VM Core

`php_vm::CompiledUnit` wraps a `php_ir::IrUnit` for execution.
`php_vm::Vm` verifies IR by default, creates a top-level `Frame` with checked
`RegisterFile` and `LocalFile` storage, and dispatches the scalar instruction
subset: `Nop`, `LoadConst`, `Move`, `LoadLocal`, `StoreLocal`, `Binary`,
`Compare`, `Unary`, `Cast`, `Discard`, `Echo`, `CallFunction`, and `Return`.
Unsupported instructions and terminators produce an `unsupported` status. Invalid
registers, locals, constants, blocks, missing terminators, uninitialized
register reads, division by zero, and unsupported scalar type combinations
produce controlled runtime errors instead of panics.

`php_vm::bytecode` defines a separate dense bytecode representation for Tier 0
execution work. Rich IR remains the verified frontend/optimizer boundary and
the default interpreter continues to execute rich IR. The CLI exposes
`php-vm run --exec-format=ir|auto|bytecode`; the default is `ir`. Dense
bytecode lowers only `nop`, `load_const`, `move`, `load_local`, `store_local`,
dense scalar binary operations, scalar unary operations, comparisons, simple
direct positional user-function calls, `echo`, discard, jump terminators, and
simple non-reference returns. Unsupported instruction families are rejected by the
dense lowerer instead of being executed with alternate semantics. Strict
`bytecode` mode returns unsupported for rejected units, while `auto` falls back
to the rich-IR interpreter. The dense verifier checks register, local,
constant, jump target, block terminator, source-span side table, cache slot,
operand-shape, and source-map consistency before the executor consumes the
format.

Dense bytecode also exposes `php-vm run --superinstructions=off|on`; the
default is `off`. When enabled with bytecode execution, the selector currently
fuses adjacent `load_const` plus `echo`, `load_local` plus `echo`, and
`binary_concat` plus `echo` pairs. Fused opcodes still use the same constant,
operand, binary-concat, register-write, and echo helpers as the unfused path,
and unsupported instruction families remain rejected or fall back through the
normal execution-format policy.

Normal VM calls use a request-local frame pool for reuse-eligible plain
user-function activations. Each `Frame` records whether it may enter the pool,
and `pop_recycle()` discards non-eligible frames instead of pooling them.
Fresh-frame fallback is used for closure captures, by-reference params or
returns, generator/fiber continuations, class contexts, shared top-level locals,
try/finally bodies, and object-allocation bodies that may retain
destructor-sensitive values. Frame/register counters include
`frames_allocated`, `frames_reused`, `register_files_allocated`,
`register_files_reused`, `frame_reuse_blocked_by_reason`,
`call_frame_layout_observed`, `tiny_frame_candidates`,
`specialized_frame_hits`, `generic_frame_fallback_by_reason`,
`arg_array_avoided`, and `heap_frame_avoided`; the older
`frame_allocations` and `frame_reuses` keys remain for compatibility. FPE-19
also reports the same safe reuse boundary through request-arena counters:
`request_arena_allocations`, `request_arena_bytes`, `request_pool_resets`,
`persistent_engine_allocations`, `persistent_engine_bytes`,
`arena_fallback_allocations_by_reason`, and
`destructor_sensitive_arena_blocks`. Persistent-engine counters stay zero until
an owning immutable metadata API lands; no userland value, object, resource, or
reference state is preserved across requests.

Local slots are indexed by `LocalId` in the IR function table. The VM stores
`ValueSlot`s in the slot array so ordinary assignments stay by-value while the
Work item `BindReference` instruction can bind two local slots to one
`ReferenceCell`. Writes through either alias update the shared cell for simple
`$b =& $a` fixtures only. Undefined local reads evaluate as `Null`, emit a
PHP-formatted warning with the variable name on the output channel, and
continue execution for the covered fixtures. The fixture
`fixtures/runtime/valid/variables/undefined.php` locks this MVP behavior;
wider warning-channel parity remains outside the current subset.

Object-property references and by-reference foreach over temporary sources are
distinct known gaps. IR lowering emits stable IDs for those cases rather than
executing copied values with PHP-incompatible aliasing.

`Echo` writes to `php_runtime::OutputBuffer`. Work item string conversion is
limited to scalars: null and false emit no bytes, true emits `1`, integers and
floats use developer-facing scalar formatting, and `PhpString` writes its exact
bytes.

Work item control flow uses explicit basic blocks. `JumpIfTrue` targets the
truthy branch and uses the next physical block as the false fallthrough;
`JumpIfFalse` remains available for hand-authored IR. Work item adds `JumpIf`
with explicit true and false targets for `switch`/`match` lowering, where
fallthrough block ordering must not decide failed comparisons. The VM converts
scalar conditions through `php_runtime::convert::to_bool`.

The lowerer emits a loop target stack for `break` and `continue`; static
numeric levels are accepted within the active loop depth, while dynamic or
out-of-range levels are classified as known gaps. `switch` case bodies preserve
PHP fallthrough by jumping from an unterminated case body to the next case body.
`match` is evaluated as an expression with strict identity comparisons. When no
arm matches and no default exists, the MVP returns a deterministic runtime
error with diagnostic ID `E_PHP_VM_UNHANDLED_MATCH`; later exception support can
map that ID to PHP's `UnhandledMatchError`.

## User Function MVP

Work item executes named user functions from top-level scripts. HIR function
declarations lower to dedicated `IrFunction`s, and `CompiledUnit` builds a
normalized function-name table for deterministic lookup. Direct named calls use
`CallFunction`; dynamic calls are an explicit known gap. Arguments are evaluated
left-to-right and copied into callee parameter locals by value. Each invocation
pushes a VM `Frame`, so nested calls and small recursive functions do not depend
on global temporary state.

`return` captures the returned value for the caller. A bare `return;` returns
`Null`, which allows `??` and echo fixtures to observe PHP-like null behavior in
the scalar subset. Runtime errors raised while more than one frame is active
append a `call_stack:` section listing the active functions from current frame
back to `main`; top-level-only errors keep their previous message text.

Work item adds required/optional parameter metadata, folded scalar/null/string
defaults, simple variadics, argument-count runtime diagnostics, and return-type
MVP checks. Omitted optional parameters are filled only from Semantic frontend
constant-expression candidates that folded without runtime evaluation. Variadic
parameters collect remaining positional arguments into the packed-list facade;
integer offset reads such as `$args[0]` are supported only for that facade.

Too few and too many positional arguments produce stable runtime diagnostic IDs:
`E_PHP_VM_TOO_FEW_ARGS` and `E_PHP_VM_TOO_MANY_ARGS`. Return declarations for
`int`, `float`, `string`, `bool`, `null`, `void`, `mixed`, simple class names,
and nullable wrappers are lowered into the runtime type adapter. Work item uses
that Semantic frontend type information for scalar/nullable parameter checks,
scalar/nullable/void return checks, and simple public property write checks.
Checks are exact except `int` values satisfy `float`; the full PHP
weak/strict conversion matrix is documented as a known gap. Parameter, return,
and property mismatches produce `E_PHP_VM_PARAM_TYPE_MISMATCH`,
`E_PHP_VM_RETURN_TYPE_MISMATCH`, and `E_PHP_VM_PROPERTY_TYPE_MISMATCH`.

By-reference parameters, named arguments, argument unpacking, dynamic functions,
and callable fallback behavior remain known gaps or later function slices.

## Closure MVP

Work item executes simple closure values through the existing call-frame
infrastructure. `MakeClosure` evaluates capture operands left-to-right, stores
stable capture names with cloned by-value values, and produces a
`Value::Callable` closure. `CallClosure` requires that callable value and then
enters the referenced `IrFunction` with captured locals initialized before
ordinary parameter locals.

Normal closures support explicit `use ($x)` by-value capture. Arrow functions
support implicit by-value capture of variables discovered in the HIR body,
excluding arrow parameters. Closure functions have independent local/register
storage, so ordinary local writes inside the closure do not mutate the caller's
local slots in this MVP.

`use (&$x)` is preserved in IR capture metadata and captures the source local's
reference cell at execution. Closure, dynamic string, array method, static
method, invokable-object, and first-class callable values execute through the
unified callable path for covered fixtures. `Closure::bind`, complete
namespace/import fallback, and invalid-callable edge cases remain known gaps.

## Callable And Pipe MVP

Work item introduces a runtime callable model. Simple first-class callable names
resolve to `CallableValue::UserFunction` when the compiled unit contains a
matching user function, or to `CallableValue::InternalBuiltin` for registered
entries in `php_runtime::BuiltinRegistry`. Closure values, dynamic string
callables, array method callables, static method callables, and invokable
objects are callable through the same helper for covered fixtures. Unresolved
dynamic callables remain explicit placeholder/gap variants.

The PHP 8.5 pipe operator evaluates the LHS first, evaluates or resolves the
RHS callable second, and then calls that callable with exactly one argument.
The supported RHS forms include simple user functions, selected builtins,
closures, dynamic string callables, and invokable objects. Non-callable RHS
values fail with `E_PHP_VM_PIPE_RHS_NOT_CALLABLE`. Unresolved callable names
fail with `E_PHP_VM_UNRESOLVED_CALLABLE`. Complete namespace/import fallback
and wider callable edge cases remain known gaps.

## Builtins I MVP

Work item moves internal functions into `php_runtime::BuiltinRegistry`. The
registry is deterministic, sorted by normalized name, and exposes
`InternalFunction` entries that receive `RuntimeContext`, argument values, and a
`RuntimeSourceSpan`. Direct named calls and callable execution both use this
registry, so `gettype($x)`, `print "x"`, `var_dump($x)`, and
`$x |> gettype(...)` share one implementation.

Implemented PHP-compatible MVP builtins are `print`, `gettype`, `is_int`,
`is_string`, `is_bool`, `is_null`, `is_array`, `var_dump`, `strlen`,
`strtoupper`, and `trim`. No internal assertion helper is exposed as a PHP
standard function in this work item.

`echo` remains an IR instruction instead of a builtin because it is a language
construct statement/expression-list form already represented by `Echo` and does
not need callable resolution. `print` is modeled as a builtin call because it is
an expression with a return value and the frontend construct node does not carry
a normal callable name.

`var_dump` output is stable for the implemented scalar, packed-array, and
ordered mixed-array subset and is covered by runtime fixtures and reference
diff. It does not yet attempt the full PHP formatting matrix for resources,
objects, references, recursion, string escaping, or floating-point edge cases.

## Include/Require MVP

Work item implements a root-constrained local `IncludeLoader` in `php_vm`.
`php_vm_cli run <path.php>` configures the loader to the executed file's
directory. The VM resolves relative include paths against the currently
executing file, canonicalizes the target path, rejects files outside the
configured roots, compiles the included source through the same
Frontend -> IR -> VM pipeline, and executes the included top-level function in
the current runtime output context.

The MVP shares matching top-level local slots between the including file and
the included file. This covers simple fixture programs where the including
source mentions the variable that the included file reads or writes. Full PHP
scope behavior for globals, symbol-table mutation, conditional declarations,
function/class redeclaration, include_path search, stream wrappers, URL
includes, and realpath-cache compatibility remains documented as known gaps.

## Object MVP

Work item introduces runtime `ObjectRef` values backed by encapsulated object
storage. Each object carries a stable identity, class name, and instance
property map initialized from the compiled class table. Object storage is safe
Rust state behind the `ObjectRef` API so later heap/GC work can replace the
storage strategy without changing the public value shape.

Semantic frontend class declarations lower into `IrUnit.classes`; `CompiledUnit` exposes
normalized class lookup to the VM. `NewObject` creates an instance, initializes
declared public properties to their defaults or `Null`, and calls `__construct`
when present. Public typed property writes are checked against the lowered
runtime type adapter. Constructor functions run as ordinary IR functions with
`$this` bound to the new object; constructor return values are ignored.

`FetchProperty` and `AssignProperty` support static public property names on
object operands. Two separately constructed objects keep independent property
maps, which is covered by `fixtures/runtime/valid/objects/two-objects.php`.
Unknown classes fail with `E_PHP_VM_UNKNOWN_CLASS`. Unsupported modifiers such
as private/protected/static/typed/readonly properties are rejected during IR
lowering with object-specific diagnostic IDs. General method calls,
visibility dispatch, traits, interfaces, magic methods, reflection, dynamic
properties, nullsafe property access, and class type checks remain outside this
Work item slice.

`include` and `include_once` missing-file failures emit structured warning
diagnostics and continue with `false`. `require` and `require_once` missing-file
failures are fatal runtime errors. `_once` operations use canonical paths in
the VM execution state so a file is executed at most once per VM execution.

## Scalar Conversion MVP

`php_runtime::convert` centralizes Work item conversion behavior for the VM:

- truthiness for null, bool, int, float, string, arrays, and objects;
- scalar-to-string for echo, casts, and concatenation;
- scalar-to-int and scalar-to-float for explicit casts;
- scalar-to-number for null, bool, int, float, int-string, float-string, and
  leading-numeric strings;
- strict identity and loose scalar comparison for safe MVP cases.

Numeric strings now share the Runtime semantics classifier. PHP's full warning channel,
INF/NAN spelling, resource conversions, object `__toString`, and extension
cases are documented known gaps rather than silently emulated.

## CLI Pipeline

`php_vm_cli` provides `compile`, `dump-ir`, `run`, and a reserved `compare`
command. `run <path.php>` reads the file, invokes the Semantic frontend frontend, lowers
HIR through `php_ir::lower_frontend_result()`, verifies IR, and executes through
`php_vm::Vm`. `dump-ir` prints the deterministic text IR. `compile --json`
prints diagnostics plus IR metadata for automation.

CLI exit classifications are stable: `0` for success, nonzero for compile or
frontend errors, nonzero for runtime errors, and a distinct nonzero code for
unsupported features. Diagnostics include the source file and byte span when the
frontend or lowering layer provides one; runtime diagnostics use the
normalizable JSON diagnostic stream described above.

## Compatibility Goals

Runtime prioritizes determinism, source maps, diagnostics, and differential
fixtures over performance. The VM can be slow. Correct rejection with a stable
diagnostic is better than incorrect output. Full PHP runtime compatibility is a
Runtime semantics+ goal.
