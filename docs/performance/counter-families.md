# Native telemetry families

The CLI and server expose one stable telemetry vocabulary for the mandatory
native engine. Counters are opt-in and serialized with schema version 11.

| Family | Meaning |
| --- | --- |
| `native_compile` | Compilation attempts, successes, time, code size, and descriptors. |
| `native_cache` | Persistent artifact loads, stores, misses, rejections, and rebuilds. |
| `native_execution` | Native entries, exits, and executed work. |
| `native_region` | Region compilation, entry, OSR, and region-level exits. |
| `native_call` | Direct compiled calls and native dispatch-trampoline activity. |
| `native_version` | Published baseline and specialized code generations. |
| `native_transition` | Guard exits and precise native-to-native continuation transfers. |
| `runtime_helper` | Calls through the typed runtime-helper ABI. |
| `native_value_table` | Runtime handle allocation, reuse, and high-water state. |
| `native_ssa` | Locals and registers promoted by executable value flow. |
| `native_ownership` | Compiler-selected moves, clones, escapes, and lifecycle boundaries. |
| `GC_safepoint` | Published-root and safepoint activity. |

New product counters must belong to one of these families. Detailed diagnostic
profiles may contain nested labels, but they must not recreate retired executor
or backend identities.

Helper counters use integer-indexed request scratch storage while execution is
active. Helper names, IR operations, functions, local/global reasons, value
classes, lifecycle reasons, root-mutation reasons, and slow-path reasons are
expanded into ordered JSON maps only when counters are exported. Clean runs do
not enter the timing or attribution path.

Render an attribution report from an instrumented counter file with:

```bash
nix develop -c scripts/performance/native_helper_report.py \
  --input target/path/to/counters.json \
  --label baseline
```

The generated JSON and Markdown stay under
`target/post-cutover/ssa-lifetimes/`.

Schema 11 also carries an explicitly audited linkage-and-footprint diagnostic
set. It covers direct-call eligibility and execution by target class, function
body deduplication, code/rodata/relocation/metadata bytes, loaded-artifact map
and entry-table counts, per-function stack bounds, worker stack reservation,
frame-arena high-water use, and bounded inlining/tail-call outcomes. These
fields retain the names used by the linkage and footprint reports instead of
creating product execution-mode families; additions require updating the
native product-surface and default-profile audits' exact allowlists.
