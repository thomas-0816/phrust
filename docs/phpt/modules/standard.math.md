# standard.math

- Priority: 14
- Selected manifest: `tests/phpt/manifests/modules/standard.math.selected.jsonl`
- Current counts: 14 PASS, 11 SKIP, 146 FAIL, 0 BORK from 171 corpus candidates

## Scope

- math and numeric standard builtins

## Non-Scope

- operator conversion semantics

## Relevant PHPT Paths

- `ext/standard/tests/math/tanh_variation.phpt`
- `ext/standard/tests/math/tanh_basiclong_64bit.phpt`
- `ext/standard/tests/math/tanh_basic.phpt`
- `ext/standard/tests/math/tan_variation.phpt`
- `ext/standard/tests/math/tan_basiclong_64bit.phpt`
- `ext/standard/tests/math/tan_basic.phpt`
- `ext/standard/tests/math/sqrt_basiclong_64bit.phpt`
- `ext/standard/tests/math/sinh_variation.phpt`
- `ext/standard/tests/math/sinh_basiclong_64bit.phpt`
- `ext/standard/tests/math/sinh_basic.phpt`
- `ext/standard/tests/math/sin_variation.phpt`
- `ext/standard/tests/math/sin_basiclong_64bit.phpt`
- `ext/standard/tests/math/sin_basic.phpt`
- `ext/standard/tests/math/round_variation1.phpt`
- `ext/standard/tests/math/round_valid_rounding_mode.phpt`
- `ext/standard/tests/math/round_prerounding.phpt`
- `ext/standard/tests/math/round_modes_zeros.phpt`
- `ext/standard/tests/math/round_modes_ceiling_and_floor.phpt`
- `ext/standard/tests/math/round_modes.phpt`
- `ext/standard/tests/math/round_large_exp.phpt`
- `ext/standard/tests/math/round_gh12143_optimize_round.phpt`
- `ext/standard/tests/math/round_gh12143_expand_rounding_target.phpt`
- `ext/standard/tests/math/round_gh12143_4.phpt`
- `ext/standard/tests/math/round_gh12143_3.phpt`
- `ext/standard/tests/math/round_gh12143_2.phpt`
- `ext/standard/tests/math/round_gh12143_1.phpt`
- `ext/standard/tests/math/round_bug71201.phpt`
- `ext/standard/tests/math/round_basiclong_64bit.phpt`
- `ext/standard/tests/math/round_basic.phpt`
- `ext/standard/tests/math/round_RoundingMode.phpt`
- `ext/standard/tests/math/round.phpt`
- `ext/standard/tests/math/rad2deg_variation.phpt`
- `ext/standard/tests/math/rad2deg_basiclong_64bit.phpt`
- `ext/standard/tests/math/rad2deg_basic.phpt`
- `ext/standard/tests/math/pow_variation2.phpt`
- `ext/standard/tests/math/pow_variation1_64bit.phpt`
- `ext/standard/tests/math/pow_variation1.phpt`
- `ext/standard/tests/math/pow_divisionbyzero.phpt`
- `ext/standard/tests/math/pow_basiclong_64bit.phpt`
- `ext/standard/tests/math/pow_basic_64bit.phpt`

## Relevant php-src Source Areas

- `ext/standard/tests/math/`

## Target Gates

- `nix develop -c just phpt-module MODULE=standard.math`

## Known Gaps

- `runtime-error-or-diagnostic`: 128
- `runtime-unsupported-feature`: 18
- `runtime-output-mismatch`: 17

## Next Step

Use php-src arginfo and Reference PHP for edge-case numeric behavior.
