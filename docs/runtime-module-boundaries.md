# Runtime Module Boundaries

`php_runtime` owns PHP-visible runtime state and service abstractions. It should
remain usable by the VM, executor, server, and `php_std` through explicit facade
imports rather than ad hoc root imports.

## Module Groups

| Group | Modules | Ownership |
| --- | --- | --- |
| Values and containers | `array`, `string`, `types`, `value`, `reference`, `convert` | PHP value representation, conversion, references, COW-visible helpers |
| Request and IO services | `context`, `output`, `error_output`, `diagnostic`, `resource`, `ini`, `globals`, `autoload`, `status` | Request-local state, streams/resources, output, diagnostics, globals, include/autoload metadata |
| Object and control-flow state | `object`, `callable`, `generator`, `fiber`, `gc` | Runtime object metadata, callables, suspension state, debug GC roots |
| Builtin service layer | `builtins`, `datetime`, `pcre`, `serialization`, `session`, `tokenizer`, `xml`, `phar`, `sqlite`, `db` | Shared state and helpers used by standard-library functions |
| Instrumentation and integration | `jit_array`, `layout_stats`, `numeric_string` | JIT/runtime ABI helpers, counters, and measurement-only metadata |

Every top-level runtime module must be represented in this table. New modules
need an ownership group before they are added to `crates/php_runtime/src/lib.rs`.

## Service Boundary

`BuiltinContext` is the request-scoped service hub for standard-library
implementation code. Builtins should prefer typed accessors on `BuiltinContext`
for output, diagnostics, filesystem state, JSON/PCRE state, HTTP response state,
upload state, sessions, and extension-specific state. Avoid adding new global
singletons or bypassing request-local service state.

## Public Surface

Downstream crates should import through `php_runtime::api` unless they
intentionally need a debug or experimental surface documented in
`docs/api-facades.md`. Crate-root re-exports are compatibility aliases, not a
place to grow new dependencies.
