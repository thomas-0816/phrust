# Performance known gaps

The machine-readable source of truth is
`docs/known_gaps/performance.jsonl`.

| Gap ID | Area | Current gap | Closure evidence |
| --- | --- | --- | --- |
| `PERF-GAP-REAL-APPLICATION-COVERAGE` | Native compiler/runtime | Performance validation remains workload- and platform-specific. | Expand strict real-application and PHPT acceptance across supported targets while preserving PHP 8.5.7 behavior. |

Mandatory Cranelift execution, W^X ownership, native ABI validation, and the
persistent machine-code cache are established architecture, not known gaps.
