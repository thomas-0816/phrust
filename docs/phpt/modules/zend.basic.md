# zend.basic

- Priority: 4
- Selected manifest: `tests/phpt/manifests/modules/zend.basic.selected.jsonl`
- Current counts: 434 PASS, 40 SKIP, 3027 FAIL, 0 BORK from 3509 corpus candidates

## Scope

- top-level execution
- scalar literals
- numeric literal separators
- echo
- print
- statement sequencing
- top-level return
- top-level exit
- basic var_dump output

## Non-Scope

- dynamic variables
- objects
- extensions
- advanced type system
- exact string-to-float formatting edge cases

## Relevant PHPT Paths

- `Zend/tests/zend_strtod.phpt`
- `Zend/tests/zend_ini/zend_ini_parse_uquantity_overflow.phpt`
- `Zend/tests/zend_ini/zend_ini_parse_quantity_zero.phpt`
- `Zend/tests/zend_ini/zend_ini_parse_quantity_overflow.phpt`
- `Zend/tests/zend_ini/zend_ini_parse_quantity_octal_prefixes.phpt`
- `Zend/tests/zend_ini/zend_ini_parse_quantity_ini_setting_error.phpt`
- `Zend/tests/zend_ini/zend_ini_parse_quantity_ini_set_error.phpt`
- `Zend/tests/zend_ini/zend_ini_parse_quantity_hex_prefixes.phpt`
- `Zend/tests/zend_ini/zend_ini_parse_quantity_error.phpt`
- `Zend/tests/zend_ini/zend_ini_parse_quantity_binary_prefixes.phpt`
- `Zend/tests/zend_ini/zend_ini_parse_quantity.phpt`
- `Zend/tests/zend_ini/oss_fuzz_428983568.phpt`
- `Zend/tests/zend_ini/gh16892.phpt`
- `Zend/tests/zend_ini/gh16886.phpt`
- `Zend/tests/zend_ini/gh11876.phpt`
- `Zend/tests/xor_001.phpt`
- `Zend/tests/write_property_ref_overwrite_return.phpt`
- `Zend/tests/weakrefs/weakrefs_debug_dump.phpt`
- `Zend/tests/weakrefs/weakrefs_006.phpt`
- `Zend/tests/weakrefs/weakrefs_005.phpt`
- `Zend/tests/weakrefs/weakrefs_004.phpt`
- `Zend/tests/weakrefs/weakrefs_003.phpt`
- `Zend/tests/weakrefs/weakrefs_002.phpt`
- `Zend/tests/weakrefs/weakrefs_001.phpt`
- `Zend/tests/weakrefs/weakmap_weakness.phpt`
- `Zend/tests/weakrefs/weakmap_nested.phpt`
- `Zend/tests/weakrefs/weakmap_multiple_weakrefs.phpt`
- `Zend/tests/weakrefs/weakmap_iteration.phpt`
- `Zend/tests/weakrefs/weakmap_error_conditions.phpt`
- `Zend/tests/weakrefs/weakmap_dtor_exception.phpt`
- `Zend/tests/weakrefs/weakmap_basic_map_behavior.phpt`
- `Zend/tests/weakrefs/notify.phpt`
- `Zend/tests/weakrefs/gh20073.phpt`
- `Zend/tests/weakrefs/gh17442_2.phpt`
- `Zend/tests/weakrefs/gh17442_1.phpt`
- `Zend/tests/weakrefs/gh13612.phpt`
- `Zend/tests/weakrefs/gh10043-016.phpt`
- `Zend/tests/weakrefs/gh10043-015.phpt`
- `Zend/tests/weakrefs/gh10043-014.phpt`
- `Zend/tests/weakrefs/gh10043-012.phpt`

## Relevant php-src Source Areas

- `Zend/tests/`
- `crates/php_vm/`
- `crates/php_runtime/`

## Target Gates

- `nix develop -c just phpt-module MODULE=zend.basic`

## Known Gaps

- `runtime-error-or-diagnostic`: 1386
- `runtime-unsupported-feature`: 1136
- `runtime-output-mismatch`: 653
- `frontend-parse-or-compile`: 43
- `runtime-timeout`: 9

## Next Step

Keep the selected zend.basic gate green while later modules expand runtime semantics.
