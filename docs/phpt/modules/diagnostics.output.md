# diagnostics.output

- Priority: 6
- Selected manifest: `tests/phpt/manifests/modules/diagnostics.output.selected.jsonl`
- Last focused run: 2026-06-28
- Current counts: 6 PASS, 0 SKIP, 0 FAIL, 0 BORK from 6 selected
  generated candidates

## Scope

- warnings
- notices
- fatal formatting
- display_errors
- output channels

## Non-Scope

- exact wording for intentionally unsupported extensions

## Relevant PHPT Paths

- `tests/phpt/generated/diagnostics.output/array-to-string-warning.phpt`
- `tests/phpt/generated/diagnostics.output/builtin-arity-error.phpt`
- `tests/phpt/generated/diagnostics.output/builtin-type-error.phpt`
- `tests/phpt/generated/diagnostics.output/include-missing-warning.phpt`
- `tests/phpt/generated/diagnostics.output/invalid-operand-type-error.phpt`
- `tests/phpt/generated/diagnostics.output/undefined-variable-warning.phpt`

## Relevant php-src Source Areas

- `crates/php_runtime/`
- `crates/php_vm/`

## Target Gates

- `nix develop -c just phpt-module MODULE=diagnostics.output`
- `nix develop -c just verify-runtime`

Last focused run on 2026-06-28:

- Selected module gate:
  `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src TARGET_PHP=target/debug/phrust-php PHPT_TARGET_MODE=php-cli PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=diagnostics.output`
  - Reference: 6 PASS, 0 SKIP, 0 FAIL, 0 BORK
  - Target: 6 PASS, 0 SKIP, 0 FAIL, 0 BORK
- Related selected gates:
  - `zend.basic`: 10 PASS, 0 non-green
  - `operators.conversions`: 4 PASS, 0 non-green
- Source integrity: 24475 php-src manifest entries verified

Covered selected-gate behavior:

- warning formatting and continuation for undefined variables
- warning formatting and continuation for array-to-string conversion
- warning formatting and continuation for missing `include`
- catchable builtin arity errors
- catchable builtin type errors
- catchable invalid operand `TypeError`

## Known Gaps

Full PHP diagnostic wording parity remains broader than this gate. Exact
messages, stack traces, and channels for unsupported extensions and advanced
runtime features are tracked in the owning feature modules rather than in this
cross-cutting selected diagnostics gate.

## Next Step

Keep the selected diagnostic channel gate green while broader PHP wording parity
is expanded through affected feature modules.
