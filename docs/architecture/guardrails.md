# Architecture Guardrails

The post-remediation architecture is enforced by `just architecture-guardrails`.
`just source-integrity` includes that target, so the same checks run in the
existing CI aggregate.

The gate derives production Rust files and Cargo edges from the tracked source
tree. Generated, vendored, test, and tooling files are classified separately.
It enforces these contracts:

1. Runtime core does not depend on extension backends, VM cache owners do not
   import frontend/optimizer layers, and only outer entry points depend on the
   server.
2. Production file sizes cannot exceed the checked post-remediation inventory.
   Large-file exceptions are path-specific and have zero implicit growth.
3. `php_runtime` and `php_vm` expose only stable `api`, `experimental`, and the
   documented `debug` facade; root imports and re-exports require review.
4. New module-wide lint suppression is rejected. Unreasoned item-local
   `too_many_arguments`, `result_large_err`, and `unsafe_code` allowances cannot
   exceed the checked debt baseline.
5. Structural source reconstruction in semantics and IR must use the reviewed
   source-text inventory.
6. Rendered diagnostics cannot be parsed for control flow outside the ratcheted
   inventory.
7. Pointer addresses cannot become logical integer identities outside the
   ratcheted inventory.
8. Generated metadata, runtime registration, and reflection surfaces must match
   the canonical extension descriptors.
9. Migrated extension state has one typed owner and borrowed narrow service
   views, without fallback-owned duplicate state.
10. Inline-cache byte budgets, warmed lookup performance, optimizer snapshot
    instrumentation, and include-cache invalidation coverage remain executable
    contracts.

Run `scripts/verify/architecture_guardrails.py --self-test` to execute one
temporary violating fixture for every rule class. Failures report the rule,
file or symbol, old baseline, and required remediation.

The warmed lookup regression gate is `just inline-cache-lookup-benchmark-gate`.
It compares dense-ID lookup with the coordinate lookup control in the same
Criterion process, avoiding an absolute cross-machine timing threshold.
