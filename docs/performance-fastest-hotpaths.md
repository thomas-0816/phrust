# Fastest Engine Hotpaths

This report ranks engine work from VM counters and existing performance artifacts. Wall-clock timings are not used for priority.

## Inputs

| Input | Status | Records | Reason |
| --- | --- | ---: | --- |
| `benchmark_smoke` | `ok` | 16 |  |
| `framework_smoke` | `ok` | 9 |  |
| `acceleration_matrix` | `ok` | 127 |  |
| `counter_json` | `ok` | 607 |  |

## Ranked Areas

| Rank | Area | Counter events | Class | Top evidence | Next evidence |
| ---: | --- | ---: | --- | --- | --- |
| 1 | Optimizer And Runtime Allocation | 2153457 | `very_high` | `target/performance/app-flows/runs/collection_transform_pagination/phrust-release-fast/iter-instrumented.counters.json` via `counter-json` (185754) | Destructor, reference, COW, output-order, and verifier-bracketed optimizer fixtures. |
| 2 | Dispatch | 738078 | `very_high` | `target/performance/app-flows/runs/collection_transform_pagination/phrust-release-fast/iter-instrumented.counters.json` via `counter-json` (53160) | Dense opcode, quickening, and superinstruction A/B fixtures. |
| 3 | Calls And Builtins | 114480 | `very_high` | `target/performance/app-flows/runs/collection_transform_pagination/phrust-release-fast/iter-instrumented.counters.json` via `counter-json` (12299) | Call-shape, by-reference, named-argument, method visibility, and stdlib diffs. |
| 4 | Strings And Output | 84306 | `very_high` | `target/performance/app-flows/runs/template_render_escape/phrust-release-fast/iter-instrumented.counters.json` via `counter-json` (4179) | Output-buffer callback, object conversion, binary string, and diagnostic-order fixtures. |
| 5 | Arrays And Foreach | 62133 | `very_high` | `target/performance/app-flows/runs/collection_transform_pagination/phrust-release-fast/iter-instrumented.counters.json` via `counter-json` (11808) | Packed, mixed, numeric-string key, by-ref foreach, COW, mutation, and order fixtures. |
| 6 | Properties And Methods | 13564 | `very_high` | `target/performance/app-flows/runs/model_hydration_json/phrust-release-fast/iter-instrumented.counters.json` via `counter-json` (2400) | Visibility, typed/readonly properties, magic, hooks, dynamic properties, and override fixtures. |
| 7 | Native And JIT Candidates | 551 | `medium` | `target/performance/app-flows/runs/dependency_container_resolution/phrust-fast-preset/iter-0.counters.json` via `counter-json` (116) | Feature-gated JIT rows with interpreter fallback, compile-budget, and side-exit reports. |
| 8 | Include And Autoload | 137 | `medium` | `target/performance/inline-cache-smoke/inline_cache-include-path-cache.on.counters.json` via `counter-json` (15) | Include/require warning order, stream-wrapper rejection, generated autoload, and invalidation fixtures. |

## Optional Profilers

- `callgrind`: `skipped` at `target/performance/callgrind/summary.json`: Callgrind is only supported by this gate on Linux; host is Darwin
- `linux-perf`: `skipped` at `target/performance/perf*.json`: no Linux perf artifact found

## Correctness Policy

The report is advisory for prioritization only; any optimization must still prove stdout, stderr/runtime diagnostics, exit status, fallback counters, and focused fixture parity.
