# WordPress Root Request Profiling

Date: 2026-07-05

Phrust request profiling is an opt-in server mode for attributing real web
request work without writing anything to PHP stdout. It is intended for
WordPress root-page performance work where the entry script and include compile
caches are already warm and most time is inside VM execution.

## Enable Request Profiles

Start `phrust-server` with a profile output directory:

```bash
nix develop -c cargo run -p php_server --bin phrust-server -- \
  --docroot /path/to/wordpress \
  --front-controller index.php \
  --request-profile target/performance/server/request-profile
```

The equivalent environment variable is:

```bash
PHRUST_REQUEST_PROFILE=target/performance/server/request-profile
```

Setting `PHRUST_REQUEST_PROFILE=1` writes profiles under
`target/performance/server/request-profile`. Profiling is disabled by default.
When enabled, the server collects VM counters for the request and serializes one
pretty JSON file at request end. The existing `--perf-trace` JSONL output is
unchanged.

## Real WordPress Helper

For an optional local WordPress root profile, set `PHRUST_WORDPRESS_DIR` and run:

```bash
nix develop -c just wordpress-root-profile
```

The helper skips cleanly when the local WordPress checkout or `phrust-server`
binary is unavailable. When prerequisites are present, it starts
`phrust-server`, requests `/`, and writes artifacts under:

```text
target/performance/wordpress-root-profile/
```

Each run contains request-profile JSON files plus `summary.json`,
`summary.md`, `perf-trace.jsonl`, and `server.log`.

## Real WordPress Benchmark Gate

For an optional root-page benchmark that records wall time, TTFB, correctness
checks, selected server metrics, and the latest request-profile attribution, run:

```bash
nix develop -c just wordpress-root-benchmark
```

The benchmark supports an already running Phrust WordPress target:

```bash
PHRUST_WORDPRESS_URL=http://127.0.0.1:18080 \
  nix develop -c just wordpress-root-benchmark
```

When the target is a port-mapped container whose WordPress site URL uses a
different host header, pass the expected host without changing the application:

```bash
scripts/performance/wordpress_root_benchmark.py \
  --url http://127.0.0.1:18081 \
  --host-header 127.0.0.1:8080
```

Or a local host docroot:

```bash
PHRUST_WORDPRESS_DOCROOT=/path/to/wordpress \
  nix develop -c just wordpress-root-benchmark
```

It skips cleanly when no usable WordPress docroot/server target exists. Output
is written under:

```text
target/performance/wordpress-root/
```

Each run writes `summary.json` and `summary.md`. When the helper launches
`phrust-server` itself, it also writes request profiles, `perf-trace.jsonl`,
and `server.log`. The optional `PHRUST_WORDPRESS_ROOT_BASELINE` environment
variable points at a previous `summary.json`; latency regressions are advisory
by default and become failing only with `--strict`.

## Dense Fallback Report

After collecting a request profile, generate a dense/rich fallback report with:

```bash
nix develop -c just wordpress-dense-fallback-report
```

By default the helper reads the newest request-profile JSON under
`target/performance/wordpress-root/`,
`target/performance/wordpress-root-profile/`, or
`target/performance/server/request-profile/`. To pin an input profile:

```bash
PHRUST_REQUEST_PROFILE_JSON=target/performance/wordpress-root-profile/root-.../request.json \
  nix develop -c just wordpress-dense-fallback-report
```

It writes:

```text
target/performance/dense-fallbacks/wordpress-root.json
target/performance/dense-fallbacks/wordpress-root.md
```

The report ranks dense include-entry fallback paths and reasons, rich fallback
functions, dense call/method/function fallback reasons, and the hottest include,
function, method, and builtin boundaries. It is workload tooling only: the VM
still records generic fallback families and has no WordPress-specific behavior.

## Array Hotpath Report

After collecting a request profile, generate an array-focused report with:

```bash
nix develop -c just wordpress-array-hotpath-report
```

To pin an input profile:

```bash
PHRUST_REQUEST_PROFILE_JSON=target/performance/wordpress-root-profile/root-.../request.json \
  nix develop -c just wordpress-array-hotpath-report
```

It writes:

```text
target/performance/array-hotpaths/wordpress-root.json
target/performance/array-hotpaths/wordpress-root.md
```

The report ranks array operation families, fast-path hits, fallback reasons,
packed direct gets, mixed indexed gets, linear scan fallbacks, metadata
recomputes, observed shapes, packed/record-to-mixed transitions, foreach clone
reasons, numeric-string key cache activity, clone source families, and
array-related builtin profiles. It is workload tooling only; array counters and
fast-path reasons stay generic runtime observations.

## Call Hotpath Report

After collecting a request profile, generate a call/frame-focused report with:

```bash
nix develop -c just wordpress-call-hotpath-report
```

To pin an input profile:

```bash
PHRUST_REQUEST_PROFILE_JSON=target/performance/wordpress-root-profile/root-.../request.json \
  nix develop -c just wordpress-call-hotpath-report
```

It writes:

```text
target/performance/call-hotpaths/wordpress-root.json
target/performance/call-hotpaths/wordpress-root.md
```

The report ranks function, method, and builtin boundary time; function/method/
builtin inline-cache activity; dense call fallback reasons; builtin fast-stub
and intrinsic hits/misses; frame allocations and reuse; observed frame layouts;
specialized/direct frame hits and fallbacks; argument-vector allocation
avoidance; by-reference argument fallbacks; and clone source families. It is
workload tooling only; call and frame counters stay generic VM observations.

## Persistent Metadata Report

After collecting a request profile, generate a persistent-metadata-focused
report with:

```bash
nix develop -c just wordpress-persistent-metadata-report
```

To pin an input profile:

```bash
PHRUST_REQUEST_PROFILE_JSON=target/performance/wordpress-root-profile/root-.../request.json \
  nix develop -c just wordpress-persistent-metadata-report
```

It writes:

```text
target/performance/persistent-metadata/wordpress-root.json
target/performance/persistent-metadata/wordpress-root.md
```

The report summarizes persistent-engine allocation volume, request-arena
allocation volume, include resolution/compile hits and misses, quickening
specialization and dequickening, arena fallback reasons, dense include fallback
reasons, clone source families, array metadata recomputes, and hot includes.
It is workload tooling only; these are generic VM metadata counters and do not
claim OPcache parity.

## Native Region Report

After collecting a request profile, generate a native/JIT-region-focused report
with:

```bash
nix develop -c just wordpress-native-region-report
```

To pin an input profile:

```bash
PHRUST_REQUEST_PROFILE_JSON=target/performance/wordpress-root-profile/root-.../request.json \
  nix develop -c just wordpress-native-region-report
```

It writes:

```text
target/performance/native-regions/wordpress-root.json
target/performance/native-regions/wordpress-root.md
```

The report summarizes native candidates, compiled regions, executions, compile
budget rejections, JIT compile/execution/side-exit counters, dense versus rich
instruction mix, dense fallback reasons, array/object blockers, and the hottest
include/function/method/builtin boundaries. It is workload tooling only; native
eligibility, side-exit, and blocker reasons stay generic VM observations.

## JSON Shape

Every profile has these top-level fields:

- `schema_version`: profile schema version.
- `request`: request id, method, URI, script path, HTTP status, cache hit,
  response bytes, and diagnostic count.
- `phases_nanos`: server phase timings, including `php_vm_execution`.
- `attribution`: grouped VM attribution counters.

The `attribution` object includes:

- `summary_counters`: flat counters also used by the performance trace.
- `execution`: rich and dense instruction counts, opcode families, and dense
  execution families, including `rich_fallback_functions_by_name` for dense
  functions that resumed in the rich VM.
- `includes`: include counts, include-cache counters, include fallback reasons,
  `dense_include_entry_fallback_by_path`, and `include_profiles_by_path`
  entries with `count`, `inclusive_nanos`, `exclusive_nanos`,
  `rich_instructions`, and `dense_instructions`.
- `calls`: function, method, internal dispatch, inline-cache, intrinsic, and
  builtin fast-stub counters. `function_profiles_by_name`,
  `method_profiles_by_name`, and `builtin_profiles_by_name` expose the same
  boundary timing and instruction fields as include profiles.
- `arrays`: array fast-path hits, fallback reasons, shape observations, foreach
  clone reasons, packed/record transition reasons, and
  `operation_profiles_by_family` timings for generic dimension and foreach
  operation families.
- `objects`: object/property inline-cache counters, fallback reasons, and
  `operation_profiles_by_family` timings for generic property operation
  families.
- `clones`: value clone, array-handle clone, COW, reference-cell, semantic clone
  reason counters, and runtime-observed source-family maps:
  `value_clone_by_source_family`, `array_handle_clone_by_source_family`,
  `cow_separation_by_source_family`, and
  `reference_cell_creation_by_source_family`.
- `output`: output buffer append/flush counters, slow append reasons, and
  `operation_profiles_by_family` timings for echo and output-buffer builtin
  operation families.
- `metadata`: persistent-engine allocation counters, request-arena counters,
  include-cache counters, arena fallback reasons, and quickening
  specialization/dequickening counters.
- `native`: native/JIT candidate, rejection, side-exit, and execution counters.

Current profiles aggregate by existing VM counter families, runtime layout
source scopes, and request-local timing scopes. They expose clone, fallback,
dense/rich, array, object, builtin, include, output, and native sources tracked
by the VM. Include/function/method/builtin profiles are sampled at VM execution
boundaries, so they are useful for ranking source sites by inclusive time,
exclusive time, and instruction deltas. Array/object/output operation profiles
are generic family timings across rich IR and dense-bytecode execution funnels.
Clone source-family maps are runtime-observed event attribution for the current
VM source scope, with `unattributed` used when a lower-level runtime event had
no active VM source. These maps intentionally identify operation classes such
as `call_argument_snapshot`, `array_element_read`, `object_property_read`,
`output_string_conversion`, and `by_ref_argument_binding`, not workload-specific
function or file names. DB and extension work is reported through builtin timing
and existing fallback/counter maps where the runtime observes it.

## Reading A Profile

For WordPress root latency, start with:

1. `phases_nanos.php_vm_execution`: confirms whether the VM still dominates.
2. `attribution.summary_counters.vm_value_clones`,
   `attribution.clones.value_clone_by_reason`, and
   `attribution.clones.value_clone_by_source_family`: identifies clone-heavy
   families and the VM source scopes that actually produced runtime clones.
3. `attribution.execution.include_rich_instructions` and
   `attribution.execution.dense_bytecode_instructions`: shows dense/rich split.
4. `attribution.includes.*fallback*`: identifies include-entry and rich fallback
   reasons.
5. `attribution.includes.include_profiles_by_path`: ranks include bodies by
   time and rich/dense instruction deltas.
6. `attribution.calls.function_profiles_by_name`,
   `attribution.calls.method_profiles_by_name`, and
   `attribution.calls.builtin_profiles_by_name`: rank PHP call and builtin
   dispatch boundaries by time.
7. `attribution.execution.rich_fallback_functions_by_name` and
   `attribution.includes.dense_include_entry_fallback_by_path`: name the dense
   function/include bodies that still execute through the rich VM.
8. `attribution.arrays.operation_profiles_by_family`,
   `attribution.objects.operation_profiles_by_family`, and
   `attribution.output.operation_profiles_by_family`: rank generic operation
   families by inclusive VM time.
9. `attribution.arrays.array_fast_path_fallback_by_reason`: shows why
   array-heavy WordPress paths leave fast representations.
10. `attribution.calls.builtin_fast_stub_fallback_by_reason`: shows builtin
   dispatch fast-stub misses.
11. `attribution.native.native_eligibility_rejections_by_reason`: explains why
   native/JIT execution is not covering the request.

Do not commit generated files under `target/`.
