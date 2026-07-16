# Performance Optimizer Passes

Performance optimizer work is a correctness-preserving IR rewrite layer. It runs
after frontend lowering and before VM execution, and every optimized unit must
still pass the IR verifier. `--opt-level=0` is the semantic baseline.

## CLI Surface

```bash
nix develop -c cargo build -p php_vm_cli --bin php-vm
nix develop -c target/debug/php-vm dump-ir tests/fixtures/performance/optimizer/arithmetic.php
nix develop -c target/debug/php-vm run --opt-level=0 tests/fixtures/performance/optimizer/arithmetic.php
nix develop -c target/debug/php-vm run --opt-level=1 tests/fixtures/performance/optimizer/arithmetic.php
nix develop -c target/debug/php-vm run --opt-level=2 tests/fixtures/performance/optimizer/arithmetic.php
```

`--opt-level=0` skips the optimizer pipeline. `--opt-level=1` and
`--opt-level=2` run the Performance pass pipeline and are tested against the
baseline by `optimizer-diff`, `performance-regression`, and
`default-profile-smoke`.

## Implemented Pass Families

| Pass family | Purpose | Required guardrail |
| --- | --- | --- |
| No-op/direct pipeline plumbing | Establishes pass reports and verifier boundaries. | Must preserve IR exactly. |
| Constant folding | Folds safe scalar constants such as integer arithmetic, boolean not, and string concatenation. | Must avoid overflow, diagnostics, conversions, references, and observable PHP behavior changes. |
| Peepholes | Removes no-op instructions and self moves where safe. | Must keep effectful and register-defining instructions. |
| CFG cleanup | Simplifies constant branches, forwards empty blocks, and trims unreachable empty tails. | Must preserve exception boundaries, source maps, and verifier-valid definitions. |
| Literal pooling/string interning | Reuses immutable literals through existing IR/runtime mechanisms. | Must not expose identity changes through PHP-visible mutation or references. |

The optimizing native tier additionally runs transformations over the
authoritative executable `RegionGraph`. Its SCCP-style scalar folding, constant
branch selection, scalar GVN/CSE, ownership-aware DCE, and conservative
LICM/GCM placement alter the instructions consumed by Cranelift. PHP-specific
CFG, dominance-frontier phi planning, and value-flow facts then select direct
CLIF for proven integer/scalar operations and promoted locals. Overflow,
coercion, diagnostics, references, globals, magic behavior, and unknown value
classes retain typed runtime slow paths.

## Verification And Rollback Ownership

`PassPipeline` is the sole owner of optimizer verification and rollback. It
verifies the incoming unit once, then verifies each pass whose transaction has
an observable change exactly once before commit. Pass implementations do not
call the verifier and read-only or net-unchanged passes are not verified again.

Each pass receives a `PassTransaction`. Mutation is copy-on-first-write at the
smallest supported rollback scope: one function, the constant pool, the class
table, or the global constant table. A pass error or failed verification drops
or explicitly rolls back that scope. There is no complete `IrUnit` clone or
serialization fallback in the transaction path.

The JSON optimizer report exposes both a typed `scope` and deterministic
statistics for every pass:

- `scope.functions` and `scope.blocks` identify indexed IR regions.
- `scope.constants` and `scope.metadata` identify unit tables.
- `scope.source_mappings_may_change` is false because the transaction does not
  expose source-map mutation.
- `scope_snapshots` and `snapshot_bytes` report snapshot count and estimated
  retained bytes. The byte count includes owned function instructions, locals,
  and string constants; it is an estimate rather than allocator telemetry.
- `verifier_calls` is zero for unchanged passes and one for changed passes.

Pass order remains the order declared by `PassPipeline::performance`. Reports,
scope indexes, metadata names, and statistics use stable vector or ordered-map
representations.

## Architecture And Measurement Baseline

The pre-transaction implementation kept optimizer pipeline, passes, analyses,
reports, and tests in one 3,280-line `lib.rs`. Its production section contained
seven unconditional full-unit clone sites and eight verifier call sites,
including pass-local verification plus pipeline phase verification. The current
layout keeps the public surface in `lib.rs`, with separate `pipeline.rs`,
`transaction.rs`, `reports.rs`, and `passes/` modules. The largest production
optimizer module is below 500 lines.

`just optimizer-diff` is the retained regression benchmark for small optimizer
fixtures, selected medium runtime/stdlib fixtures, and the configured
application corpus. It compares output, diagnostics, exit status, and runtime
counters across O0, O1, and O2. The same recipe runs the `php_optimizer` unit
tests, which enforce these ownership invariants directly:

- a no-op pass creates a snapshot or reports a mutation scope;
- an unchanged pass invokes the verifier;
- a changed pass invokes the verifier other than exactly once.

The differential result is written to
`target/performance/optimizer-diff/summary.json`. The native-only product CLI no
longer emits internal optimizer reports, so the gate does not launch duplicate
compile-only processes for every fixture. Use
`just architecture-performance-baseline --scope compile --scope benchmarks`
when wall time, peak RSS, and incremental compile measurements are required;
the compiler row runs `optimizer-diff`, while the WordPress row runs the pinned
WordPress root benchmark. Generated reports stay under `target/` and are not
committed.

## Validation

Use the narrow gate while iterating:

```bash
nix develop -c cargo test -p php_optimizer
nix develop -c cargo test -p php_ir verify --lib
nix develop -c just optimizer-diff
nix develop -c just native-ssa-ratchet
```

Before finishing optimizer work, run:

```bash
nix develop -c just performance-regression
nix develop -c just verify-performance
```

`optimizer-diff` compares opt levels 0, 1, and 2 across optimizer fixtures and
prints clear differences if stdout, stderr, exit status, diagnostics, or
optimizer transaction invariants diverge.

## Troubleshooting

- If a fixture changes output under `--opt-level=1` or `2`, first rerun with
  `--opt-level=0` and inspect `dump-ir` before and after the suspected pass.
- If the verifier rejects optimized IR, keep the verifier failure as the root
  signal and fix the pass; do not weaken verifier rules for convenience.
- If a fold looks profitable but can change warnings, exceptions, references,
  COW, destructors, magic methods, or conversion order, leave it unfused and add
  a fixture documenting the blocked case.
- If `perf-compare` does not show a speedup, do not treat that as an optimizer
  correctness failure. Performance wall-clock budgets are advisory.
