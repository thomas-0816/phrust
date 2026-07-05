# WordPress Root Performance Follow-Up

Date: 2026-07-05

Scope: real WordPress root page from
`/Volumes/CrucialMusic/src/phrust_branches/phrustwordpress`, served by this
Phrust checkout through the Docker demo image rebuilt from the current tree.

This report is intentionally conservative on speed claims. The current work
removed one concrete dense-bytecode fallback and fixed the Docker build context,
but the rebuilt live WordPress root request is still far slower than native PHP.

## Measurement Setup

The rebuilt container reported:

```text
rustc_host=aarch64-unknown-linux-gnu
rustflags=-C target-cpu=native -C target-feature=+neon
server_features=php_vm/jit-cranelift
```

Runtime knobs were:

```text
PHRUST_ENGINE_PRESET=experimental-jit
PHRUST_SCRIPT_CACHE_PRELOAD=1
```

For attribution, the same image was restarted with:

```text
PHRUST_REQUEST_PROFILE=/var/tmp/phrust-profiles
```

Requests used `Host: 127.0.0.1:8080` because the demo WordPress site URL is
configured that way while the service is port-mapped to `127.0.0.1:18080`.

## Docker Build Context Fix

The first attempt to rebuild the Docker demo from this checkout was stopped
after it had already transferred more than 2.6 GB of source context. The Phrust
additional Docker context had no `.dockerignore`, so generated build outputs
under `target/` were being sent to Docker.

The new root `.dockerignore` excludes generated/build/runtime artifacts such as
`target/`, `.git/`, local caches, local env files, screenshots, and
`third_party/`. The next build transferred about 45.8 MB of Phrust source
context and completed the release `php_server` build in about 3m20s.

## Live Result

Fresh rebuilt live benchmark:

```text
PHRUST_WORDPRESS_URL=http://127.0.0.1:18080
PHRUST_WORDPRESS_HOST_HEADER=127.0.0.1:8080
PHRUST_METRICS_TOKEN=dev-metrics-token
PHRUST_WORDPRESS_ROOT_SAMPLES=3
PHRUST_WORDPRESS_ROOT_WARMUPS=2
nix develop -c just wordpress-root-benchmark
```

Artifact:

```text
target/performance/wordpress-root/root-20260705T190135Z-3d6e2c89da4b/summary.md
```

Result:

| metric | value |
| --- | ---: |
| HTTP status | 200 |
| body bytes | 21680 |
| wall avg | 6324.184 ms |
| wall min/max | 5848.490 / 6819.233 ms |
| TTFB avg | 6324.119 ms |

The metrics delta for the same run shows that the request is dominated by VM
execution, not HTTP routing or script-cache work:

| phase | count | total time |
| --- | ---: | ---: |
| `vm_execution` | 3 | 18966.278 ms |
| `script_cache` | 3 | 0.060 ms |
| `route_resolution` | 8 | 0.081 ms |
| `response_build` | 3 | 0.028 ms |
| `request_context` | 3 | 0.036 ms |

Profiled attribution run:

```text
target/performance/wordpress-root/current-profiled-after-bind-reference-dim/request-profiles/req-00000006.json
```

That profiled request had extra counter-writing overhead and should be used for
attribution, not latency. Its phase split was:

| phase | time |
| --- | ---: |
| `php_vm_execution` | 13629.818 ms |
| `response_build` | 0.431 ms |
| `script_cache_lookup` | 0.027 ms |
| `route_resolution` | 0.024 ms |
| `request_context` | 0.010 ms |

The entry script and include caches were active:

| counter | value |
| --- | ---: |
| `entry_script_cache_hits` | 1 |
| `entry_script_cache_misses` | 0 |
| `include_resolution_hits` | 499 |
| `include_resolution_misses` | 0 |
| `include_compile_hits` | 490 |
| `include_compile_misses` | 0 |
| `include_source_reads` | 0 |

## Generic Runtime Fix Slice

The implementation stayed generic:

- Runtime layout attribution now has shared source-family constants for array
  reads/writes, reference dereference, stack/register/local moves, foreach value
  materialization, return values, output conversion, closure capture binding,
  and by-reference argument binding.
- Reference-cell conversions borrow for read-only scalar conversion instead of
  cloning through `ReferenceCell::get()`.
- Predicate-only VM paths borrow reference cells for `empty()`, scalar type
  checks, static-property `isset`/`empty`, and branch truthiness.
- Rich and dense branch execution collapse `LoadLocal rX; JumpIf* rX` when the
  loaded local is used only as a branch predicate.
- Standard-library exact scalar arguments now materialize by copying the scalar
  payload instead of cloning `Value`; exact non-scalar builtin argument
  materialization is attributed under `builtin_argument_materialization`.
- Array builtin output materialization uses the same scalar-copy rule for
  generic array transformation helpers and attributes unavoidable non-scalar
  output copies under `array_builtin_output_materialization`.
- Dense bytecode now lowers and executes generic array-dimension reference
  binding (`$array[$key] =& $local`) through the dense VM.

The previous real-root profile showed 1,408 dense call fallbacks for:

```text
BindReferenceDim { local: LocalId(1), dims: [Register(RegId(16))],
append: false, source: LocalId(2) }
```

In the new profile, that `BindReferenceDim` fallback is gone. Remaining dense
call fallbacks are now other generic PHP instruction gaps such as `InstanceOf`,
property `empty`/`isset`, dynamic property assignment, `BindReference`, dynamic
object creation, and property-dimension operations.

## Current Hotspots

Current profiled root counters:

| counter | value |
| --- | ---: |
| dense bytecode instructions | 992706 |
| rich instructions | 744161 |
| include rich instructions | 744017 |
| dense functions executed | 33394 |
| rich fallback functions executed | 359 |
| dense include attempts/successes/fallbacks | 490 / 469 / 21 |
| function calls | 104637 |
| method calls | 11469 |
| internal function dispatches | 70554 |
| value clones | 98980010 |
| array-handle clones | 27038140 |
| JIT compiled/executed | 0 / 0 |

Clone churn is still the largest visible runtime pressure:

| source | count |
| --- | ---: |
| value clones, unattributed | 93357074 |
| array-handle clones, unattributed | 25125878 |
| value clones, reference dereference | 1846588 |
| value clones, array element write | 1742780 |
| value clones, stack/register/local move | 1226997 |
| array-handle clones, reference dereference | 967256 |
| value clones, array element read | 428752 |
| array-handle clones, array element read | 326664 |

Array execution is busy and still frequently blocked by COW/reference identity:

| counter | value |
| --- | ---: |
| array operations | 196579 |
| `dim_isset` | 48800 |
| `dim_fetch` | 47805 |
| `foreach_next` | 50133 |
| `dim_assign` | 23997 |
| packed append fast hits | 21734 |
| record-shape fetch hits | 14031 |
| packed foreach fast hits | 7396 |
| packed int fetch hits | 2886 |
| `cow_or_reference` array fast-path fallbacks | 21828 |
| array metadata recomputes | 3514 |

Call dispatch remains expensive:

| counter | value |
| --- | ---: |
| function calls | 104637 |
| method calls | 11469 |
| internal dispatches | 70554 |
| frame allocations | 16809 |
| frame reuses | 27680 |
| argument vectors avoided | 37396 |

The inclusive call report is dominated by generic dynamic callable paths:

| callable | count | inclusive time |
| --- | ---: | ---: |
| `call_user_func_array` | 760 | 20550.826 ms |
| `call_user_func` | 1681 | 3647.987 ms |

Native execution is still absent even with the ARM/NEON release build:

| counter | value |
| --- | ---: |
| native candidates | 869 |
| native compiled regions | 0 |
| native executions | 0 |
| JIT compile attempts | 2 |
| JIT compiled | 0 |
| JIT executed | 0 |
| top native rejection | `call_shape` (836) |
| JIT blacklist | `compile_errors` (2) |

Persistent metadata is active for compile/source reuse and request-local
feedback, but not for reusable execution bodies:

| counter | value |
| --- | ---: |
| include compile hits/misses | 490 / 0 |
| persistent engine allocations | 0 |
| persistent engine bytes | 0 |
| quickening attempts | 1022403 |
| quickening specialized | 6530 |
| quickened executions, scalar branch | 131642 |
| quickened executions, string concat | 13829 |
| quickened executions, integer arithmetic | 8818 |
| quickened executions, packed array fetch | 2886 |

## Root Cause

The `BindReferenceDim` fix moved one real generic PHP operation into dense
bytecode, but it did not change the main execution model for the root request.
A warmed WordPress root request still:

1. Executes hundreds of include bodies per request.
2. Runs include/function/method bodies through interpreted dense and rich VM
   paths.
3. Allocates and clones tens of millions of runtime values.
4. Routes hot work through dynamic callable and method-dispatch shapes that
   prevent native regions.
5. Uses PHP reference, object, property, array, dynamic-call, and declaration
   semantics that are not yet represented in a fast executable form.

The practical consequence is that bytecode caching and include compile caching
are working, but they only remove parsing/lowering/source-read cost. They do not
turn the warmed request into a reusable native or compact VM execution body.

## Real Fix Direction

The next fixes must remain generic PHP/runtime fixes:

1. Remove the remaining massive unattributed clone sources by wrapping lower
   runtime helper boundaries in clone-source attribution and replacing the
   highest-volume generic `Value` clone paths with borrow, move, or
   copy-on-write preserving APIs.
2. Expand dense bytecode coverage for `InstanceOf`, property `isset`/`empty`,
   dynamic property read/write, class constant/static property fetch, object
   construction, declarations, and remaining reference-binding forms.
3. Build a generic dynamic callable fast path for stable `call_user_func` and
   `call_user_func_array` shapes, including direct frame setup and argument
   spreading without repeated vector/value cloning.
4. Make method dispatch cross-unit and class-context aware without giving up to
   rich VM when the resolved class and method are stable.
5. Convert persistent metadata from request-local feedback templates into
   immutable, guarded, reusable execution metadata for function/method/include
   bodies while keeping request state out of the persistent layer.
6. Only after those blockers shrink, enable native regions for stable
   call/array/property/basic-block shapes. The current ARM/NEON build is active,
   but there is still no native region to execute.

None of these require WordPress-specific runtime behavior. WordPress remains a
real workload and regression gate, not a compatibility patch target.

## Validation

Focused checks:

- `nix develop -c cargo test -p php_runtime array_builtin_materialization`
- `nix develop -c cargo test -p php_runtime array_slice_merge_and_transform_builtins_work`
- `nix develop -c cargo test -p php_std arginfo`
- `nix develop -c cargo test -p php_vm bytecode_lowering_covers_array_dim_reference_binding`
- `nix develop -c cargo test -p php_vm dense_bytecode_auto_executes_array_dim_reference_binding`
- `nix develop -c cargo check -p php_runtime -p php_std -p php_vm -p php_server`
- `nix develop -c just verify-performance` passed before the final helper-only
  `operand_truthy_at_frame` move out of `vm/mod.rs`
- After that helper move:
  `nix develop -c cargo check -p php_vm`,
  `nix develop -c cargo test -p php_vm bytecode_lowering_covers_array_dim_reference_binding`,
  and
  `nix develop -c cargo test -p php_vm dense_bytecode_auto_executes_array_dim_reference_binding`

Fresh live checks:

- `PHRUST_SOURCE_DIR=/Volumes/CrucialMusic/src/phrust_branches/phrust2 docker compose build phrust`
- `PHRUST_SOURCE_DIR=/Volumes/CrucialMusic/src/phrust_branches/phrust2 docker compose up -d phrust`
- `PHRUST_WORDPRESS_URL=http://127.0.0.1:18080 PHRUST_WORDPRESS_HOST_HEADER=127.0.0.1:8080 PHRUST_METRICS_TOKEN=dev-metrics-token PHRUST_WORDPRESS_ROOT_SAMPLES=3 PHRUST_WORDPRESS_ROOT_WARMUPS=2 nix develop -c just wordpress-root-benchmark`
- `PHRUST_REQUEST_PROFILE_JSON=target/performance/wordpress-root/current-profiled-after-bind-reference-dim/request-profiles/req-00000006.json nix develop -c sh -lc 'scripts/performance/dense_fallback_report.py && scripts/performance/clone_churn_report.py && scripts/performance/array_hotpath_report.py && scripts/performance/call_hotpath_report.py && scripts/performance/persistent_metadata_report.py && scripts/performance/native_region_report.py'`
