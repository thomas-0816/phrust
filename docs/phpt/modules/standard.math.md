# standard.math

- Priority: 14
- Selected manifest: `tests/phpt/manifests/modules/standard.math.selected.jsonl`
- Prompt 16.1 baseline: 99 PASS, 11 SKIP, 62 FAIL, 0 BORK from 172 corpus candidates
- Prompt 16.9 focused gate: 161 PASS, 11 SKIP, 0 FAIL, 0 BORK

## Scope

- Math and numeric standard builtins covered by the selected module gate

## Non-Scope

- General operator conversion semantics
- Parser/IR gaps surfaced by math PHPTs but owned by other layers

## Relevant PHPT Paths

- `ext/standard/tests/math/`
- `tests/phpt/generated/standard.math/core-math-edge-cases.phpt`

## Relevant Source Areas

- `crates/php_runtime/src/builtins/modules/math.rs`
- `crates/php_runtime/src/convert.rs`
- `crates/php_std/src/constants.rs`
- `crates/php_std/src/lib.rs`

## Target Gates

- `nix develop -c cargo test -p php_runtime`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=standard.math`
- `nix develop -c just verify-stdlib`

## Prompt 16 Evidence

- Added math-module implementations and registrations for trigonometric,
  hyperbolic, logarithmic, exponential, base-conversion, `pi`, `fpow`,
  `getrandmax`, and expanded `round` mode support.
- Added standard math constants, `PHP_ROUND_HALF_*`, and `STR_PAD_*`
  constants.
- Added `tests/phpt/generated/standard.math/core-math-edge-cases.phpt`.
- Latest focused target run: PASS, 172 selected PHPTs with 161 PASS and
  11 SKIP.

## Known Gaps

- The 11 SKIPs are preserved as selected-gate skips, not failures.
- Broader numeric edge cases and cross-layer blockers remain backlog work when
  expanding beyond the focused selected module.
