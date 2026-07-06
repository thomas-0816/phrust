# Oracle Gap Closure Loop

The oracle loop turns php-src metadata, reference CLI behavior, generated
runtime probes, PHPT results, and application smoke failures into a prioritized
gap queue. Use it as the discovery path before relying on WordPress smoke.

## Workflow

1. Run the cheap oracle gate:
   ```bash
   REFERENCE_PHP=$REFERENCE_PHP \
     nix develop -c just oracle-smoke
   ```
2. Inspect the top gap family in `target/oracle/gap-report-summary.md` or
   `target/oracle/gap-report.json`.
3. Generate the next family-level prompt:
   ```bash
   nix develop -c just oracle-next-gap-prompt
   ```
4. Implement a generic fix in the owning layer. Do not special-case the
   generated fixture.
5. Add or promote focused fixtures that prove the behavior against
   `REFERENCE_PHP`.
6. Rerun the focused gate named by the prompt.
7. Rerun:
   ```bash
   nix develop -c just oracle-gap-report --check
   ```
8. Remove, narrow, or update known-gap rows only after the focused fixture is
   green and the report confirms the gap family shrank.

`oracle-gap-report` defaults to the deterministic cheap input set: the PHP
source API index plus generated oracle probe reports under `target/oracle/`.
Use `nix develop -c just oracle-gap-report --full` only after intentionally
refreshing the broader runtime, stdlib, PHPT, and application-smoke reports
that live under `target/`.

## Ratchets

- `oracle-smoke` fails on unclassified oracle failures.
- `oracle-gap-report --check` fails known-gap rows that lack fixture, source,
  layer, priority, or reason.
- P0/P1 open counts cannot exceed `tests/oracle/gap-report-baseline.json`
  unless the baseline is updated explicitly with reviewed evidence.
- WordPress smoke is final compatibility evidence. Source/reference probes are
  the primary discovery mechanism.

## Seed Families

These are the initial families used to prove the loop during WordPress bring-up:

| Family | Owning layer | Reference source | Focused gate |
| --- | --- | --- | --- |
| API class/function surface | `php_std/php_runtime` | php-src generated arginfo and reference reflection | `just verify-stdlib` |
| Reflection parameter metadata | `php_std/php_runtime` | reference `ReflectionFunction` / arginfo | `just diff-stdlib` |
| Callable dispatch by reference | `php_runtime/php_vm` | generated callback probes | `just oracle-probe-smoke` |
| Dynamic static method calls | `php_semantics/php_ir` and `php_vm` | generated frontend-lowering probes | `just oracle-probe-smoke` |
| Destructuring holes and source offsets | `php_semantics/php_ir` and `php_vm` | generated frontend-lowering probes | `just oracle-probe-smoke` |

For each family, fix the generic layer first, then let the report and probe
manifest decide whether the known-gap entry can be removed or downgraded.
