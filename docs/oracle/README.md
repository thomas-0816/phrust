# Oracle Workflow

The oracle workflow turns the pinned PHP 8.5.7 source tree and reference CLI
into a deterministic compatibility queue for Phrust.

Start with the architecture and implementation plan:

- [PHP source oracle gap closure](../php-source-oracle-gap-closure.md)
- [Oracle gap closure loop](codex-loop.md)

Generated artifacts:

- `target/oracle/api/php-source-api-symbols.jsonl`
- `target/oracle/api/php-source-api-summary.md`
- `fixtures/runtime_semantics/oracle_generated/{smoke,full}/*.php`
- `tests/oracle/manifests/generated-probes.jsonl`
- `target/oracle/probes/{smoke,full}/runtime-semantics-diff-report.json`
- `target/oracle/gap-report.json`
- `docs/oracle/gap-report-summary.md`

Commands:

- `just oracle-api-index`
- `just oracle-api-summary`
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
under the same strict-reference rule.

`oracle-next-gap-prompt` reads `target/oracle/gap-report.json` and emits one
family-level implementation prompt for the highest-priority open gap family.

Use `PHP_SRC_DIR` and `REFERENCE_PHP` to point at the read-only pinned PHP
oracle. For local development in this workspace, the intended source oracle is
`/Volumes/CrucialMusic/src/phrust/third_party/php-src`.

Current committed summary:

- [Oracle gap report summary](gap-report-summary.md)
