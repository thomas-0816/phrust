# Dependency Units

Dependency units are metadata-only groups for future module-level optimization.
The planner lives in `php_vm` because it consumes VM-ready `IrUnit` metadata,
include behavior, autoload lookup observations, and cache-facing fingerprints.
It does not change execution, cache hits, or bytecode layout.

This planner is distinct from the executable `php_ir::CompilationSession`.
Include compilation now uses that session to own immutable source files, stable
file IDs, typed trait-declaration requests, and explicit dependency edges. Each
source is parsed and analyzed independently, dependency files are lowered in a
deterministic graph order into one linked `IrUnit`, and every IR span continues
to reference its original file-table entry. No PHP source is concatenated or
rewritten. The include layer supplies the declaration resolver; the compiler
does not infer Composer or PSR mappings by scanning source lines or directory
trees. `IncludeLoader::with_compilation_dependency` supplies normalized
declaration-to-path metadata directly. Production executor loaders also install
an executor-owned resolver that evaluates Composer's generated
`autoload_classmap.php` and `autoload_psr4.php` through typed HIR. It checks only
the conventional `vendor/composer` location in the requester's ancestor roots;
it never searches source trees for declarations. An unmapped declaration
remains unresolved even when a same-named PHP file exists below an allowed
root, and a mapped file that does not declare the requested trait fails closed.

The linked unit retains entry-file `strict_types` as a compatibility field and
also records `file_strict_types[FileId]` for dependency-owned functions. Exact
dependency and Composer metadata-file identities are included in the
compiled-include cache key, so edits, same-metadata rewrites, map changes, and
atomic replacements invalidate the root artifact. The deterministic resolver
fingerprint is also part of the compiler identity, preventing artifacts
compiled with different resolver implementations or direct mappings from
sharing a cache hit. Composer-resolved units are linked at compile time but
remain inactive until the runtime autoload protocol requests that declaration;
this preserves registered callback order and side effects without reparsing or
recompiling the dependency.

`performance/multi_file_trait_compile` in `php_bench` measures the cold linked
include path with an explicit trait mapping. The existing include-cache and
framework benchmarks continue to cover warm-cache and request behavior.

## Graph Shape

The planner emits `DependencyUnitId`, `DependencyUnit`, `DependencyEdge`,
`DependencyGraph`, and `InvalidationReason` records.

Unit families:

- `file`: IR file-table entry plus best-effort content, mtime, and inode
  fingerprint.
- `function`: user function, closure, method body, or top-level body.
- `class`: class, interface, enum, parent/interface metadata, methods, and
  properties.
- `method` and `property`: class-owned members with edges back to the class and
  implementation function where available.
- `constant` and `literal`: global/class constant values and executable literal
  constants or arrays.
- `include_expression`: static or dynamic include/require sites.
- `lookup`: class, method, function, callable, or constant lookup sites.
- `autoload_resolver`: autoload map/version metadata.
- `configuration`: compile or runtime configuration components.

Core edge families:

- `file -> function/class/constant`
- `class -> methods/properties`
- `function/property/constant -> literal constants/arrays`
- `function -> include expression`
- `include expression -> observed or static target file`
- `lookup -> autoload resolver`
- `file/unit -> configuration component`

Static include targets are recorded only when the include operand is a constant
string. Runtime include target sets and negative lookup observations are explicit
planner inputs so request-time behavior can enrich the report without making the
IR planner execute filesystem or autoload logic.

`just dependency-units-smoke` exercises the in-memory planner and its
deterministic report digest through focused Rust tests. The former product CLI
reporter was removed during the native-only cutover; generated planner reports
are not part of the product surface. The digest can still be supplied as an
extra cache-fingerprint component by future cache integration work.

## Invalidation Constraints

Symlinks: file units store both source-table path and canonical path when the
filesystem is available. Reuse must invalidate when symlink targets change, not
only when the source spelling is unchanged.

`include_path`: include edges depend on include-path ordering, current working
directory, and including-file directory. Include-path changes invalidate both
positive and negative include observations.

Case sensitivity: target files can resolve differently on case-sensitive and
case-insensitive filesystems. File and include edges carry a
`case_sensitivity_changed` invalidation reason where a path spelling is part of
resolution.

Generated files: generated source may appear, disappear, or change without a
stable repository identity. Generated-file edges are never treated as permanent
native-code cache proof; they need content and metadata revalidation.

PHAR and archives: PHAR support is future-facing for dependency units. Archive
member identity must include the archive fingerprint and member path rather than
only a local extracted path.

Development vs production mode: development mode should revalidate content and
metadata aggressively. Production mode may rely on explicit deployment
fingerprints, but those fingerprints must cover file content, autoload maps,
configuration, and include-path policy.

Negative lookup cache invalidation: missing include or autoload lookup results
are not trusted forever. They carry `negative_lookup_expired` and must be
invalidated by include-path, autoload-map, generated-file, or deployment
fingerprint changes.

## Persistent Feedback And Module Compilation

Persistent feedback already records request-local observations and validates
them with a cache fingerprint. Dependency units are the graph counterpart: they
describe which immutable engine-owned groups the feedback depends on.

Future module-level compilation can use this graph to:

- group multiple functions and class metadata into one immutable unit;
- attach persistent feedback to dependency units rather than one script path;
- reason about autoload and include invalidation before cross-function
  optimization;
- reject or replan native code when any dependency edge becomes stale.

The dependency-unit planner intentionally stops at planning and reporting. The
separate compilation session links declarations needed to compile an include;
it does not introduce cross-function optimization, persistent native code
caching, filesystem watchers, or SAPI/server behavior.
