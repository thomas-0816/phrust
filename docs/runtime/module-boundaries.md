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
| Core services | `datetime`, `serialization`, `session` | Backend-free runtime semantics shared by the VM and extensions |
| Legacy full-runtime integration | `builtins`, `pcre`, `tokenizer`, `xml`, `xml_backend`, `phar`, `sqlite`, `db` | Feature-gated compatibility surface while extension implementations migrate outward |
| Extension contract | `extension`, `source_span` | Registration descriptors, call ABI metadata, capabilities, state factories, and shared source locations |
| Instrumentation and integration | `jit_array`, `layout_stats`, `numeric_string` | JIT/runtime ABI helpers, counters, and measurement-only metadata |

Every top-level runtime module must be represented in this table. New modules
need an ownership group before they are added to `crates/php_runtime/src/lib.rs`.

## Service Boundary

`php_runtime --no-default-features` is the mechanically enforced runtime-core
configuration. The default `full-runtime` feature preserves the compatibility
surface while remaining implementations migrate to `php_extensions`.

`BuiltinContext` remains a temporary adapter for unmigrated implementation code.
New extension ownership belongs in `php_extensions`; runtime core must not import
that crate. Prompt 08 owns decomposition into narrow call services and typed
request-state slots.

## Public Surface

Downstream crates should import through `php_runtime::api` unless they
intentionally need a debug or experimental surface documented in
`docs/api-facades.md`. Crate-root re-exports are compatibility aliases, not a
place to grow new dependencies.
