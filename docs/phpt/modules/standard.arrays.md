# standard.arrays

- Priority: 12
- Selected manifest: `tests/phpt/manifests/modules/standard.arrays.selected.jsonl`
- Current counts: 86 PASS, 0 SKIP, 734 FAIL, 0 BORK from 821 corpus candidates

## Scope

- ext/standard array builtins

## Non-Scope

- array COW engine work

## Relevant PHPT Paths

- `ext/standard/tests/strings/substr_replace_array_unset.phpt`
- `ext/standard/tests/strings/substr_replace_array.phpt`
- `ext/standard/tests/strings/strip_tags_array.phpt`
- `ext/standard/tests/strings/pack_arrays.phpt`
- `ext/standard/tests/serialize/serialization_arrays_005.phpt`
- `ext/standard/tests/serialize/serialization_arrays_004.phpt`
- `ext/standard/tests/serialize/serialization_arrays_003.phpt`
- `ext/standard/tests/serialize/serialization_arrays_002.phpt`
- `ext/standard/tests/serialize/serialization_arrays_001.phpt`
- `ext/standard/tests/network/setcookie_array_option_error.phpt`
- `ext/standard/tests/http/http_response_header_deprecated_nested_op_arrays.phpt`
- `ext/standard/tests/http/http_response_header_deprecated_multiple_op_arrays.phpt`
- `ext/standard/tests/hrtime/hrtime_array.phpt`
- `ext/standard/tests/forward_static_call_array.phpt`
- `ext/standard/tests/array/var_export3.phpt`
- `ext/standard/tests/array/var_export2.phpt`
- `ext/standard/tests/array/var_export.phpt`
- `ext/standard/tests/array/sort/usort_variation9.phpt`
- `ext/standard/tests/array/sort/usort_variation7.phpt`
- `ext/standard/tests/array/sort/usort_variation6.phpt`
- `ext/standard/tests/array/sort/usort_variation5.phpt`
- `ext/standard/tests/array/sort/usort_variation3.phpt`
- `ext/standard/tests/array/sort/usort_variation11.phpt`
- `ext/standard/tests/array/sort/usort_stability.phpt`
- `ext/standard/tests/array/sort/user_sort_basics.phpt`
- `ext/standard/tests/array/sort/uksort_basic.phpt`
- `ext/standard/tests/array/sort/uasort_variation7.phpt`
- `ext/standard/tests/array/sort/uasort_variation5.phpt`
- `ext/standard/tests/array/sort/uasort_variation3.phpt`
- `ext/standard/tests/array/sort/uasort_variation10.phpt`
- `ext/standard/tests/array/sort/sort_variation_escape_sequences.phpt`
- `ext/standard/tests/array/sort/sort_variation9.phpt`
- `ext/standard/tests/array/sort/sort_variation8.phpt`
- `ext/standard/tests/array/sort/sort_variation7.phpt`
- `ext/standard/tests/array/sort/sort_variation6.phpt`
- `ext/standard/tests/array/sort/sort_variation5.phpt`
- `ext/standard/tests/array/sort/sort_variation4.phpt`
- `ext/standard/tests/array/sort/sort_variation3.phpt`
- `ext/standard/tests/array/sort/sort_variation11.phpt`
- `ext/standard/tests/array/sort/sort_variation10.phpt`

## Relevant php-src Source Areas

- `ext/standard/tests/array/`

## Target Gates

- `nix develop -c just phpt-module MODULE=standard.arrays`

## Known Gaps

- `runtime-error-or-diagnostic`: 471
- `runtime-unsupported-feature`: 208
- `runtime-output-mismatch`: 56

## Next Step

Implement array builtins after array data model gaps are closed.
