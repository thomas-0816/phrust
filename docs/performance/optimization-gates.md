# Native optimization gates

All optimization work targets the mandatory Cranelift pipeline. A change is
admissible only when baseline and default preserve PHP-visible output,
diagnostics, exit status, side-effect order, native continuation state, helper
ABI compatibility, and cache identity.

| Class | Policy | Required evidence |
| --- | --- | --- |
| Baseline lowering | Required | Exhaustive IR manifest, helper ABI audit, and native-entry tests. |
| Speculative specialization | Allowed in `default` | Guards, precise state reconstruction, native transitions, and focused reference fixtures. |
| OSR and compiled calls | Allowed | Published native entries, generation ownership, safepoints, and no effect replay. |
| Persistent native cache | Allowed | Identity validation, corruption rebuild, W^X, and fresh-process hit proof. |
| PHP-visible behavior change | Forbidden | Redesign outside the performance layer. |

Use `just cranelift-only-ratchet-fast` while iterating and the narrowest owning
performance fixture before broader gates. The fast ratchet uses incremental
`cutover` binaries and builds the CLI and server in one Cargo invocation.
Its recipes explicitly clear `RUSTC_WRAPPER` and set `CARGO_INCREMENTAL=1`:
the Nix shell disables Cargo incremental compilation globally for sccache,
which would otherwise override the profile setting.
`just cranelift-only-ratchet` remains the final architectural contract over
canonical release binaries and is included separately by `just ci-local`.

## Retained source-line lookup

Native call frames and runtime diagnostics must derive display lines through
`CompiledUnit::source_display_line`. The compiled unit retains the immutable
source snapshot and its `LineIndex`, so a hot callback must not reread a PHP
file or scan from byte zero on every invocation. Byte spans remain the source
of truth; the cached line is display metadata only.

The WordPress callback tranche that removed the per-frame file read and linear
newline scan measured the following instrumentation-free concurrency-1 result:

| Run | Samples | p50 | p95 | Throughput |
| --- | ---: | ---: | ---: | ---: |
| Before | 30 | 19,197.397 ms | 19,860.774 ms | 0.05229 req/s |
| Retained line index | 30 | 16,561.675 ms | 17,495.742 ms | 0.05977 req/s |

That is a 13.73% p50 improvement, an 11.91% p95 improvement, and a 14.32%
throughput improvement. The strict report remains timing-inconclusive because
the independently measured PHP-FPM p50 control moved by +14.31%, beyond the
chosen 10% control bound. A matching three-sample smoke was timing-eligible and
measured p50 -13.08%, p95 -11.90%, throughput +14.44%, with PHP-FPM control
drift of +2.66%. Reports remain under
`target/performance/wordpress-root/source-line-index-c1-{smoke-r2,strict}/`.

Linux sampling was unavailable on the measurement host because
`kernel.perf_event_paranoid=4` rejected all `perf` events. GDB sampling of the
profiling build identified `native_backtrace_frame` scanning source bytes below
recursive native callback dispatch; the focused regression test proves display
lines continue to use the retained compile-time source after the source file is
replaced.

## Clone-free request-root scans

Exclusive native-helper telemetry identified object release as the second
largest WordPress helper category. On the same warm request, `value_release`
accounted for 6,203.8 ms across 694,361 calls; 8,855 releases traversed request
roots. The traversal previously cloned every property name and value into a
snapshot and used ordered sets even though it only needed a reachability
answer. `ObjectRef::try_any_property_value` now short-circuits over borrowed
property values, and the traversal uses request-local hash sets. The rootedness
definition, destructor timing, and cycle detection are unchanged.

The telemetry profile reduced exclusive `value_release` time from 6,203.8 ms
to 3,959.2 ms (-36.2%) and instrumented request wall time from 21,706.2 ms to
19,326.2 ms (-11.0%). Headline timing used telemetry-free release binaries and
restored the same deterministic database snapshot before each arm (SHA-256
`c25b75325ecb656d8ff9be478c13a6c1b2aeba955eaacccbd0bbaab0f008c9f9`):

| Concurrency | Baseline p50 | Current p50 | Baseline p95 | Current p95 | Throughput change |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 17,080.207 ms | 15,034.602 ms | 17,544.562 ms | 15,977.050 ms | +12.41% |
| 4 | 21,547.378 ms | 19,256.193 ms | 22,639.364 ms | 19,553.628 ms | +14.35% |
| 8 | 28,171.668 ms | 25,892.155 ms | 32,541.564 ms | 29,011.906 ms | +12.16% |

The eight-sample per-concurrency development tranche passed all WordPress/PHP
observables and selected `keep`; c1 p50 improved 11.98%, while c1/c4/c8 p95
improved 8.93%, 13.63%, and 10.85%. PHP-FPM c1 p50 moved +5.83% between the
adjacent arms. Reports are under
`target/performance/wordpress-root/object-root-visitor-snapshot-c1-c4-c8-{baseline,candidate}/`.
This is the accepted first post-cutover exploitation tranche, not a claim of
PHP-src parity or performance completion. `just object-release-root-scan`
protects scan-free unique release and its PHP-visible destructor order, plus
the rooted traversal path.
