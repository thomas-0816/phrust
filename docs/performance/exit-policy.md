# Exit-Counter-Guided Specialization Policy

This policy is request-local metadata for adaptive engine decisions. It does
not enable native recompilation, mutate persistent feedback, or make speed-only
acceptance decisions.

## Counter Audit

Current adaptive surfaces already expose the raw inputs:

| Surface | Existing counters or metadata |
| --- | --- |
| Quickening | attempts, specializations, guard hits/misses, guard failures, fallback calls, dequickens, megamorphic transitions, disabled transitions |
| Inline caches | observations, slots, hits, misses, invalidations, guard failures, fallback calls, mono/poly/megamorphic states, disabled states |
| Tiering | function entries, loop backedges, IC stability score, guard failure score, tier selections, JIT cold/hot/eager candidates, budget and blacklist rejections |
| Fallback protocol | guard fallback, cold fallback, dequicken, megamorphic, disabled events |
| JIT blacklist | side exits, guard failures, blacklisted regions, blacklist reasons, compile-budget rejections |
| Array/property/builtin fast paths | packed-array layout/bounds exits, property-load guard/layout/uninitialized exits, builtin fast-stub misses and fallback reasons |

`ExitCounterTable` unifies those observations by function id, bytecode offset
or region id, tier, exit reason, and guard kind.

## Policy States

The policy reports one of:

| State | Meaning |
| --- | --- |
| `keep_optimized` | Feedback is stable or below thresholds. |
| `dequicken` | Repeated guard failures should disable the current request-local specialization. |
| `blacklist_for_request` | Repeated wrong-class or shape failures should suppress this site for the current request. |
| `blacklist_persistently_candidate` | Strong repeated failures are report-only candidates for future persistent invalidation work. |
| `recompile_narrower_candidate` | Builtin-call guard failures might benefit from narrower future code generation. |
| `recompile_wider_candidate` | Repeated side exits might benefit from wider future code generation. |
| `unsupported` | The site should stay generic, including packed/mixed instability and megamorphic call/property sites. |

Persistent blacklisting and recompilation candidates are advisory. No persistent
feedback file is changed by this policy.

## Thresholds

The table is configured from tiering options through `ExitPolicyThresholds`:

| Threshold | Default | Use |
| --- | ---: | --- |
| `guard_failure_threshold` | 2 | Dequickening recommendation. |
| `side_exit_threshold` | 2 | Generic fallback for unstable side exits. |
| `megamorphic_threshold` | 1 | Keep megamorphic method/property sites generic. |
| `blacklist_threshold` | 3 | Request-local blacklist recommendation. |
| `recompile_candidate_threshold` | 4 | Future recompile candidate reporting. |

## JSON

Tiering stats schema version 2 adds:

```json
"exit_policy": {
  "sites": [],
  "decisions": []
}
```

Each site includes the stable key and counters for guard failures, side exits,
megamorphic transitions, generic fallbacks, and stable hits. Decisions repeat
the key with the selected policy state and reason.

## Validation

Focused unit coverage forces:

- stable optimized site => keep optimized;
- repeated type flip => dequicken;
- repeated wrong class => blacklist for request;
- packed/mixed array instability => generic fallback through `unsupported`;
- megamorphic method/property site => stay generic through `unsupported`.

Run:

```bash
nix develop -c cargo test -p php_vm exit_policy --lib
nix develop -c cargo test -p php_vm tiering --lib
nix develop -c just inline-cache-model-tests
nix develop -c just default-profile-smoke
nix develop -c just verify-performance
```
