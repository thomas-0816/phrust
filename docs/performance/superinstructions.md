# Superinstructions

Superinstructions are interpreter-only dense-bytecode fusions. They may be added
only when the fused arm preserves the unfused sequence's stdout, stderr/runtime
diagnostics, exit status, source-map/debug accounting, fallback behavior, and
counter attribution.

`--superinstructions=off` remains the generic dense-bytecode baseline.
`--superinstructions=on` is allowed only for fusions that execute the same helper
sequence or an equivalent guarded helper as the unfused arms.

Generated pair/triple rankings are local evidence, not committed
documentation. `just superinstruction-patterns` writes them under
`target/performance/superinstructions/`; `just superinstruction-smoke` verifies
the selected fusion set against focused A/B fixtures.

New fusions need focused correctness fixtures, fallback/deopt counter coverage,
and a clear reason why the fused helper does not change PHP-visible behavior.
