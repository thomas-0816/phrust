# operators.conversions

- Priority: 5
- Selected manifest: `tests/phpt/manifests/modules/operators.conversions.selected.jsonl`
- Current counts: 16 PASS, 6 SKIP, 107 FAIL, 0 BORK from 129 corpus candidates

## Scope

- arithmetic
- bitwise operators
- comparison
- boolean conversion
- numeric-string conversion
- concat
- assignment operators
- increment/decrement
- leading numeric string warnings
- object numeric casts

## Non-Scope

- array union semantics
- array/object concat beyond __toString smoke coverage
- full TypeError/Throwable catch semantics for non-numeric operands
- pipe operator
- nullsafe operator
- property hooks
- fiber error suppression
- performance-only concat stress

## Relevant PHPT Paths

- `Zend/tests/zend_operators.phpt`
- `Zend/tests/type_declarations/add_return_type.phpt`
- `Zend/tests/ternary_operator_basic.phpt`
- `Zend/tests/sub_001.phpt`
- `Zend/tests/property_hooks/override_add_set_covariant.phpt`
- `Zend/tests/property_hooks/override_add_set.phpt`
- `Zend/tests/property_hooks/override_add_get_contravariant.phpt`
- `Zend/tests/property_hooks/override_add_get.phpt`
- `Zend/tests/pipe_operator/void_return.phpt`
- `Zend/tests/pipe_operator/type_mismatch.phpt`
- `Zend/tests/pipe_operator/too_many_parameters.phpt`
- `Zend/tests/pipe_operator/precedence_ternary.phpt`
- `Zend/tests/pipe_operator/precedence_comparison.phpt`
- `Zend/tests/pipe_operator/precedence_coalesce.phpt`
- `Zend/tests/pipe_operator/precedence_addition.phpt`
- `Zend/tests/pipe_operator/prec_007.phpt`
- `Zend/tests/pipe_operator/prec_006.phpt`
- `Zend/tests/pipe_operator/prec_005.phpt`
- `Zend/tests/pipe_operator/prec_004.phpt`
- `Zend/tests/pipe_operator/prec_003.phpt`
- `Zend/tests/pipe_operator/prec_001.phpt`
- `Zend/tests/pipe_operator/oss_fuzz_439125710.phpt`
- `Zend/tests/pipe_operator/oss_fuzz_427814452.phpt`
- `Zend/tests/pipe_operator/gh18965.phpt`
- `Zend/tests/pipe_operator/generators.phpt`
- `Zend/tests/pipe_operator/exception_interruption.phpt`
- `Zend/tests/pipe_operator/complex_ordering.phpt`
- `Zend/tests/pipe_operator/call_prefer_by_ref.phpt`
- `Zend/tests/pipe_operator/call_by_ref.phpt`
- `Zend/tests/pipe_operator/ast.phpt`
- `Zend/tests/operator_unsupported_types.phpt`
- `Zend/tests/nullsafe_operator/oss_fuzz_60011_2.phpt`
- `Zend/tests/nullsafe_operator/oss_fuzz_60011_1.phpt`
- `Zend/tests/nullsafe_operator/oss-fuzz-69765.phpt`
- `Zend/tests/nullsafe_operator/gh8661.phpt`
- `Zend/tests/nullsafe_operator/constant_propagation.phpt`
- `Zend/tests/nullsafe_operator/bug81216_2.phpt`
- `Zend/tests/nullsafe_operator/bug81216.phpt`
- `Zend/tests/nullsafe_operator/040.phpt`
- `Zend/tests/nullsafe_operator/039.phpt`

## Relevant php-src Source Areas

- `Zend/tests/`
- `crates/php_runtime/`
- `crates/php_vm/`

## Target Gates

- `nix develop -c just phpt-module MODULE=operators.conversions`

## Known Gaps

- `runtime-unsupported-feature`: 78
- `runtime-error-or-diagnostic`: 32
- `runtime-output-mismatch`: 8

## Next Step

Keep the selected scalar conversion gate green while later modules expand arrays, objects, and diagnostics.
