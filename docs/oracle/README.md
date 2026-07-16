# Oracle Workflow

The oracle workflow turns the pinned PHP 8.5.7 source tree and reference CLI
into a deterministic compatibility queue for Phrust.

Start with the architecture and implementation plan:

- [PHP source oracle gap closure](php-source-gap-closure.md)
- [Oracle gap closure loop](gap-closure-loop.md)

Generated artifacts:

- `target/oracle/api/php-source-api-symbols.jsonl`
- `target/oracle/api/php-source-api-summary.md`
- `target/native-surface/{functions,methods,classes,constants}.jsonl`
- `target/native-surface/language-operations.json`
- `target/native-surface/summary.md`
- `fixtures/runtime_semantics/oracle_generated/{smoke,full}/*.php`
- `tests/oracle/manifests/generated-probes.jsonl`
- `target/oracle/probes/{smoke,full}/runtime-semantics-diff-report.json`
- `target/oracle/gap-report.json`
- `target/oracle/gap-report-summary.md`

Commands:

- `just oracle-api-index`
- `just oracle-api-summary`
- `just native-surface-inventory`
- `just oracle-probe-generate`
- `just oracle-probe-smoke`
- `just oracle-probe-full`
- `just oracle-gap-report`
- `just oracle-next-gap-prompt`
- `just oracle-smoke`
- `just verify-oracle`

`oracle-smoke` is the cheap CI-style gate: it refreshes the API index, runs the
bounded smoke probes when `REFERENCE_PHP` is available, and fails on new
unclassified oracle failures. `verify-oracle` runs the full generated probe set
under the same strict-reference rule. `oracle-gap-report` uses deterministic
oracle/API/probe inputs by default; pass `--full` only after refreshing the
broader runtime, stdlib, PHPT, and application-smoke reports under `target/`.

`oracle-next-gap-prompt` reads `target/oracle/gap-report.json` and emits one
family-level implementation prompt for the highest-priority open gap family.

`native-surface-inventory` joins the canonical API index and runtime registry
with Region IR/Cranelift source, generated probe results, and PHPT symbol
manifests. Registration alone is reported as `registered_unprobed`; headline
support counts require an executed reference-compatible probe. The generated
ranked queue is the input for native PHP surface closure waves.

Use `PHP_SRC_DIR` and `REFERENCE_PHP` to point at the read-only pinned PHP
oracle. For local development in this workspace, the intended source oracle is
`$PHP_SRC_DIR`.

The generated markdown summary is written to
`target/oracle/gap-report-summary.md`.
