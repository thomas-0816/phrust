# Include Subsystem Ownership

The VM include subsystem is a set of one-way components under
`crates/php_vm/src/include/`. The VM owns path policy, validated source
identity, caches, metrics, and the compiler port. The concrete compiler lives
in `php_executor`, which may depend on `php_vm`; the reverse dependency is
forbidden.

## Component Boundaries

| Component | Owns | Must not own |
| --- | --- | --- |
| `source` | stable opened-file reads, content identity, portable metadata | resolution, compilation, cache policy |
| `resolver` | allowed roots, include path and cwd order, canonicalization, Phar adapters | compiled artifacts, frontend work |
| `resolution_cache` | positive paths, guarded negative entries, directory validation | compiler settings or compiled units |
| `cache_freshness` | monotonic revalidation windows and entry stamps | paths, compiler settings, or cache policy |
| `compiler` | VM-facing `IncludeCompiler` port and opaque fingerprint | concrete frontend, lowering, optimizer |
| `compiled_cache` | artifact identity, dependency validation, stampede coordination | path candidate policy, server configuration |
| `compile_coordinator` | per-path compile locks and stampede wakeups | source validation or compiled artifacts |
| `metadata` | Composer-map and deployment-root observations | hidden source scanning during compilation |
| `metrics` | atomic counters and typed immutable snapshots | cache decisions |
| `cache` | stable public facade and cross-component delegation | component implementation details |

The source dependency order is:

`diagnostics/source -> resolver -> compiler/metrics -> metadata/cache_freshness ->
resolution_cache/compiled_cache/compile_coordinator -> cache`.

`include::tests::include_module_ownership_is_one_way` enforces the declared
edges, rejects frontend and optimizer imports in every production include
module, rejects public lock types, and caps the facade size.

## Lock Policy

Locks remain private implementation details. Poisoning of positive resolution,
compiled artifact, compiled lookup, and stampede locks returns
`E_PHP_VM_INCLUDE_CACHE_POISONED`. The guarded negative cache is advisory:
poisoning degrades to a miss so it cannot turn an otherwise valid include into
a runtime failure. The characterization tests cover both policies.

## Compiler Direction

`php_vm::include::IncludeCompiler` accepts a `ValidatedIncludeSource` and
returns a `CompiledInclude` containing a `CompiledUnit` and opaque
dependency identities. `php_executor::ExecutorIncludeCompiler` is the only
production implementation and owns semantic analysis, IR lowering, and
optimization. Composer and declaration mappings are explicit loader metadata;
the compiler does not infer them from source lines or scan the filesystem.
