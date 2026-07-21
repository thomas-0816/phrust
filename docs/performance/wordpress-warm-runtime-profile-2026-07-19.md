# WordPress warm-runtime profile, 2026-07-19

This report records the current cost of serving an already warmed WordPress
request. It is a host-local measurement snapshot, not a performance contract.
Raw benchmark and request-profile artifacts remain under `target/performance/`
and are not committed.

The current clean snapshot was measured from commit `2926f32b` with a clean
worktree. It includes function-on-demand compilation, bounded native
fragmentation, sparse state publication, compact native arguments, direct
string predicates, and stable-ID builtin dispatch.

## Executive result

Phrust remains CPU-bound and substantially slower than PHP-FPM 8.5.7 in warm
mode:

| Metric | Phrust | PHP-FPM 8.5.7 | Phrust / PHP |
| --- | ---: | ---: | ---: |
| p50 latency | 440.72 ms | 26.92 ms | 16.37x |
| p95 latency | 499.36 ms | 33.42 ms | 14.94x |
| Throughput, concurrency 1 | 2.22 req/s | 36.35 req/s | 0.061x |
| CPU for ten measured requests | 4.38 s | 0.23 s | 19.04x |
| Peak RSS | 646.4 MB | 137.2 MB | 4.71x |

The HTTP status, selected headers, 70,949-byte body, and body SHA-256 were
identical. The shared body hash was
`7a34e150c5304aea4744a0e4f3b4fd70c4309cca411902513478a9ba7a196072`.

At these measurements, matching PHP requires removing about 94% of Phrust's
p50 latency and 95% of its CPU work. Single-percent improvements cannot close
the gap.

## Change from the original snapshot

The first snapshot in this report measured 1,075.7 ms p50 and 2,067.6 ms p95.
The current result is materially better:

| Metric | Original snapshot | Current snapshot | Change |
| --- | ---: | ---: | ---: |
| Phrust p50 | 1,075.7 ms | 440.72 ms | -59.0% |
| Phrust p95 | 2,067.6 ms | 499.36 ms | -75.8% |
| Phrust CPU/request | 1,221 ms | about 438 ms | -64.1% |
| Phrust peak RSS | 776 MB | 646.4 MB | -16.7% |
| Runtime helper calls | 2,200,546 | 1,323,262 | -39.9% |
| Native call-frame writes | 51.34 MB | 35.36 MB | -31.1% |

This comparison spans multiple architecture tranches and is not attribution
for one patch. The clean current-vs-PHP table above is the authoritative
snapshot.

## Measurement conditions

- Host: Linux x86_64, Intel Core i7-12800H, 20 logical CPUs.
- WordPress: 6.8.3 under `target/wordpress-cutover`.
- WordPress cron disabled for deterministic request timing.
- Reference: PHP 8.5.7 FPM, Opcache enabled, Opcache JIT disabled.
- Phrust: optimized release server, default engine preset, immutable
  deployment, preloaded script cache, warm in-memory native code.
- Load: concurrency 1, five warmups, ten measured requests per engine.
- Clean timing used external process sampling only; VM counters were disabled.
- Runtime counters came from a separate diagnostic request and must not be
  used as wall-clock timings.

The ten-sample p95 is the nearest-rank maximum. Treat it as a tail signal, not
as a high-confidence production percentile. The reported 95% bootstrap ranges
were 431.69-454.38 ms for Phrust p50 and 22.80-30.34 ms for PHP p50.

The clean benchmark can be reproduced with:

```bash
nix develop -c cargo build --release -p php_server --bin phrust-server
nix develop -c scripts/performance/wordpress_root_benchmark.py \
  --mode clean \
  --wordpress-dir target/wordpress-cutover \
  --docroot target/wordpress-cutover \
  --server target/release/phrust-server \
  --host-header 127.0.0.1:37023 \
  --samples 10 \
  --warmups 5 \
  --concurrency 1 \
  --timeout-seconds 60 \
  --out-dir target/performance/docs-current-clean
```

The matching diagnostic request can be reproduced by changing `--mode clean`
to `--mode diagnostic`, using a separate output directory, and reducing the
measured sample count to one. Instrumentation increased native execution time
to about 1.52 seconds, so diagnostic helper times are ranking data only.

## Current runtime volume

The current diagnostic request crossed 1,323,262 runtime helper boundaries.
The largest families are:

| Helper family | Calls |
| --- | ---: |
| Locals, references, global binding, and lifecycle | 435,202 |
| Arrays and foreach | 531,418 |
| Function, method, constructor, and builtin calls | 123,579 |
| Properties, including semantic property operations | 109,262 |
| Scalar, comparison, cast, truthy, and string-predicate operations | 102,106 |

The highest individual counts are:

| Helper | Calls |
| --- | ---: |
| Value release | 245,687 |
| Array fetch | 241,491 |
| Array insert | 118,573 |
| Reference bind | 111,911 |
| Foreach next | 98,500 |
| Direct builtin call | 56,082 |
| Property fetch | 55,923 |
| Semantic global bind | 38,172 |
| Function call | 37,264 |
| Comparison | 33,609 |
| Array creation | 32,012 |
| Binary operation | 30,594 |
| Local fetch | 29,114 |

Execution polling is no longer a material hot path: it fell from 197,719
calls in the original snapshot to 171 real loop/safepoint polls.

At PHP's measured 26.92 ms p50, the average budget would be about 20 ns for
each existing helper boundary before doing any PHP-visible work. Most of the
remaining boundaries must disappear, not merely become slightly cheaper.

## Value and ownership traffic

One diagnostic request recorded:

- 484,557 value-table allocations;
- 96,226 value-table slot reuses;
- a 251,855-slot value-table high-water mark;
- 245,687 release helpers, 96,342 of which reached zero; and
- 67,497 ownership escapes.

Release reasons were 143,275 temporaries, 88,670 arguments, 11,474 stores,
2,144 frame cleanups, and 124 helper results. The counters recorded no
separate ownership clones or retains on the current native path, but PHP
`Value` cloning, handle encoding, drops, and allocation remain visible inside
runtime operations.

This is now a more important structural target than another isolated builtin
intrinsic. Arrays, properties, foreach, and calls still repeatedly leave
native SSA, materialize Rust runtime values, and publish new handles.

## Call dispatch and frame traffic

The current request recorded:

- 233,308 runtime-mediated call sites;
- 136,393 calls classified as direct;
- 96,915 calls classified as dynamic;
- 24,270 same-unit direct calls;
- 36,815 cross-unit direct calls;
- 20,754 executed monomorphic method calls out of 22,002 eligible calls;
- 35.36 MB of native call transport; and
- 1.02 MB of separately allocated call-argument data.

Stable-ID builtin dispatch removed the complete generic `JitNativeCallFrame`
from 56,082 positional by-value builtin calls. Relative to the immediately
preceding diagnostic snapshot, call transport fell from 43,516,936 bytes to
35,360,296 bytes: 8,156,640 bytes, or 18.7%, per request. The focused native
fixture fell from about 512 KB to 32 KB of argument transport, a 93.75%
reduction.

The corresponding clean WordPress p50 moved from 450.75 ms to 440.72 ms,
about 2.2%. That is a real reduction, but it also demonstrates why further
per-builtin work is not the primary strategy.

Frequent dynamic classifications remain omitted required arguments (46,595),
by-reference shapes (24,281), unpublished targets (16,958), extra positional
arguments (10,462), and omitted array defaults (5,133). These are diagnostic
classifications; some describe conservative call-shape metadata rather than
an inherently dynamic PHP target.

The next call tranche must remove the shared runtime boundary for cross-unit
and monomorphic method calls while preserving native values and ownership
across the call. Replacing one resolver lookup at a time will not be enough.

## Compilation and tiering

The diagnostic warm request reported:

- zero compile attempts;
- zero compile time; and
- zero newly published versions.

Foreground Cranelift and regalloc2 work therefore does not explain the current
440.72 ms median. Function-on-demand and bounded compilation remain required
for cold-start stability, but warm performance work must now target execution
shape.

## Current conclusion

The original dominant root-membership and instruction-polling costs have been
substantially reduced. The warm bottleneck has shifted to repeated native-to-
Rust transitions and runtime value materialization:

1. array fetch/insert/foreach operations;
2. zero-reaching releases and request-arena churn;
3. reference and property operations;
4. cross-unit, method, and dynamic call binding; and
5. scalar/string operations that still leave native SSA.

The remaining gap cannot be closed by a sequence of one-percent helper
specializations. A successful tranche must remove a large shared cost block
and demonstrate simultaneous reductions in helper calls, value allocations,
call-frame bytes, CPU/request, and warm p50 against PHP.
