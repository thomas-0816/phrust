# Performance Autoload Lookup Cache

The performance layer provides a request-local inline cache for conservative class-like
autoload lookups. The cache covers class, interface, trait, and enum lookup
metadata and is disabled when inline caches are off.

## Guard Model

The cache key records:

- normalized class-like name
- lookup kind: class, interface, trait, or enum
- whether autoload is enabled for the lookup
- current autoload recursion depth
- include_path configuration string
- Composer map fingerprint when a future runtime exposes one

The cache entry is separately guarded by explicit epochs:

- autoload stack epoch
- class table epoch
- include_path/config epoch

Standard library Composer smoke fixtures register PHP callbacks and load Composer-style
maps as ordinary PHP arrays, so the VM does not yet receive a stable Composer
map object to fingerprint. In that case the fingerprint guard is `None`, while
autoload registration, class table changes, and include_path changes still
invalidate cached lookups.

The same guarded lookup path is used for unresolved static class-like access,
including class constants, static properties, static methods, and static-method
callables. Static lookups still recheck the live class table after autoload
callback execution before installing or accepting a positive cache hit.

## Cached Results

Positive results may be reused only while all guards and epochs match. The VM
rechecks the direct class table or internal extension registry before accepting
a positive hit.

Negative results are installed only when reusing them cannot suppress visible
autoload side effects: lookups with autoload disabled, or lookups with no
registered autoload callbacks. When autoload callbacks are present and autoload
is enabled, misses are not negatively cached.

The cache never stores included file contents, autoload callback execution, or
Composer files autoload side effects.

## Counters

The Performance counter JSON includes:

- `autoload_class_lookup_ic_hits`
- `autoload_class_lookup_ic_misses`
- `autoload_class_lookup_ic_invalidations`
- `autoload_class_lookup_ic_guard_failures`
- `autoload_graph_hits`
- `autoload_graph_misses`
- `negative_lookup_hits`
- `invalidations_by_reason.autoload_lookup_epoch_or_guard`
- `invalidations_by_reason.autoload_positive_target_missing`

The retained cache data model is covered by `inline-cache-model-tests` during
the native-only cutover. Product-native lookup counters and hit-rate gates must
be re-established by Prompt 16 before this historical evidence is used for a
new performance claim.

## Bootstrap Lookup Coverage (status evidence)

The web-bootstrap lookup families the optimization gates mark
`EVIDENCE_GATE` are implemented and epoch-guarded end to end:
include-path resolution ICs (`include_path_ic_*`), include-once results
(`include_once_skips` plus the include graph), autoload class-lookup ICs
(`autoload_class_lookup_ic_*`), and class-constant/static-property
metadata ICs (`class_static_ic_*`, slot family
`inline_cache_class_constant_static_property_slots`). On a repeated
bootstrap loop (`fixtures/runtime_semantics/includes/
bootstrap-lookup-cache-paths.php`) the steady state resolves 29/30
includes and 58/61 class-constant/static-property lookups from ICs while
staying byte-exact against the reference engine on both presets.

Negative autoload lookups are deliberately not cached: the reference
engine re-invokes registered autoloaders for every unknown-class probe,
so repeated `class_exists` misses must keep calling the loader. The
`negative_lookup_hits` counter only fires for the side-effect-safe graph
cases documented above.
