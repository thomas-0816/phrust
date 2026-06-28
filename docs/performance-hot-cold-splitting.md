# Hot/Cold Semantic Splitting

Date: 2026-06-28.

This audit tracks the current hot/cold boundary for interpreter fast paths. The
policy is conservative: the VM may keep exact fast guards in hot opcode
handlers, but diagnostics, conversions, magic behavior, references, COW,
destructors, fibers, generators, include/autoload semantics, and stdlib
arginfo remain on the existing generic semantic helpers.

## Audit

| Area | Hot exact case | Guard | Cold fallback helper | Diagnostics path | Blockers |
| --- | --- | --- | --- | --- | --- |
| Arithmetic | Integer and safe numeric-string arithmetic already classified by the runtime numeric-string model. | Exact scalar types, canonical numeric-string metadata, no overflow, and no warning-sensitive leading numeric string. | Shared runtime scalar conversion and arithmetic helpers. | Runtime arithmetic diagnostics and TypeError paths remain generic. | Leading numeric warnings, precision, overflow, division/modulo edge behavior, references, and object conversion. |
| Concat | String-string concat and preallocated concat with scalar inputs. | Both operands already strings, or scalar conversions that cannot warn or call userland. | `concat_fallback_reason` and `concat_operand_fallback_reason` classify the slow semantic reason before the shared generic concat path runs. | Array conversion warning, object `__toString`, resource conversion, callable/uninitialized errors stay generic. | Userland `__toString`, references, arrays, resources, callables, uninitialized values, and capacity overflow. |
| Compare | Exact scalar compares and numeric-string classification are runtime-owned. | Identical primitive classes or cached numeric-string classes that do not need warning-sensitive handling. | Shared `php_runtime::compare`, `equal`, and conversion helpers. | Comparison diagnostics stay with runtime conversion semantics. | Object comparison, arrays, resources, references, precision, and PHP loose-comparison edge cases. |
| Array fetch/assign | Packed read/append, by-value foreach, record-like/small-map read, and packed-int reduction helpers. | Shape kind, packed length/bounds, mutation epoch, no COW/reference poison, key class, and element summary. | Array helper fallback maps record `array.*` reasons through `record_array_fast_path_fallback`. | Undefined key, illegal string offset, key coercion, COW/reference, and order semantics remain generic. | References, COW writes, by-reference foreach, numeric-string ambiguity, mutation during iteration, mixed layout, and insertion order. |
| Property fetch/assign | Guarded declared-property IC load/store for public and in-scope declared slots. | Receiver class id, layout epoch, property slot/name, visibility context, type metadata, readonly state, hook/magic flags, and reference-slot absence. | Property IC fallback maps now also feed `slow_path_calls_by_reason` as `property_fetch.*` and `property_assign.*`. | Visibility, typed property, readonly, magic, hook, dynamic property, and uninitialized diagnostics stay generic. | Magic methods, property hooks, readonly/init-only state, typed validation, dynamic properties, references, and class epoch changes. |
| Calls/builtins | Function/method ICs, exact builtin stubs, intrinsic stubs, and tiny leaf user-function frames. | Stable call target, exact positional arguments, no by-ref/named/variadic shape for fast frames, and exact builtin arity/type for stubs. | Generic call path remains authoritative; builtin fallback classifiers feed `builtin_stub.*` and `builtin_intrinsic.*`; generic frame fallback feeds specialized-frame reason maps. | Argument binding, TypeError/ValueError, by-reference warnings, reflection/call context, and return-type checks stay generic. | Named args, variadics, by-ref params/returns, closures, methods with class context, generators, fibers, reflection, and userland state. |
| Echo/output | Exact bytes, null/bool/int/string echo batching and fast appends. | No object/resource/array/callable/uninitialized conversion, no buffer callback boundary, no branch/call side effect. | `OutputStats::slow_appends_by_reason` is folded into `slow_path_calls_by_reason` as `output.*`. | Array warnings, resource formatting, object `__toString`, callback unsupported behavior, and conversion errors stay generic. | Output-buffer callbacks, userland `__toString`, resources, arrays, callables, uninitialized values, and warning order. |
| Include/autoload | Include/autoload dependency graph hits, include-path IC hits, negative autoload lookup hits. | Stable path/autoload graph metadata, no stream wrapper, no missing path, no file fingerprint invalidation. | Path-semantics fallback maps now also feed `slow_path_calls_by_reason` as `include_autoload.*`; include diagnostic formatting helpers are cold. | Missing include/require warning/fatal output and include_path text stay in existing include failure helpers. | Stream wrappers, missing paths, mutable files, include_path order, once semantics, autoload side effects, and Composer map fingerprints. |

## Implemented Split

The current FPE-27 implementation avoids semantic rewrites. It adds one
cross-family counter map, `slow_path_calls_by_reason`, and feeds it from the
already-owned fallback points for concat, arrays, property fetch/assign,
builtin stubs, intrinsic stubs, output, include/autoload, and JIT helper
slow-path calls. Existing detailed maps remain intact.

The code marks only already-isolated fallback classifiers/diagnostic helpers as
cold:

| Helper | Reason |
| --- | --- |
| `include_failure` | Builds include/require diagnostics and output after the hot path has failed. |
| `emit_include_failure_output` | Formats PHP-visible warning/fatal output for failed includes. |
| `include_failure_target_and_reason` | Parses include diagnostic payloads for display text. |
| `fast_builtin_stub_fallback_reason` | Classifies why an exact builtin stub must use generic stdlib semantics. |
| `concat_fallback_reason` | Classifies non-string concat fallback before generic concat semantics. |
| `concat_operand_fallback_reason` | Classifies per-operand concat conversion blockers. |

## Tests

Existing edge-semantics tests now assert the aggregate slow-path map alongside
their original output and diagnostic expectations:

| Test coverage | Protected behavior |
| --- | --- |
| Output object, array, and resource conversion | `__toString` side-effect order, array warning text, resource formatting, and `output.*` slow reasons. |
| Concat scalar and object fallback | Scalar conversion and object `__toString` order with `concat.*` slow reasons. |
| Missing include/require | Include warning continuation, require fatal output order, and `include_autoload.missing_path`. |
| Counter unit coverage | Empty JSON schema, property fetch/assign, arrays, builtins, intrinsics, include/autoload, and JIT slow-path aggregation. |

## Code Size Evidence

No stable instruction-count artifact is available in the default local gate.
`verify-performance` still reports callgrind as skipped on Darwin, so this
report does not make wall-clock or instruction-count closure claims. The source
delta is intentionally small: hot handlers keep using existing exact guards,
slow helper attribution is centralized in `VmCounters`, and cold attributes are
limited to fallback classifiers and diagnostic formatting helpers that were
already outside the fast semantic path.

Future closure should add platform-specific code-size or instruction-count
evidence when a deterministic profiler artifact is available.
