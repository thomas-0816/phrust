# objects.classes

- Priority: 10
- Selected manifest: `tests/phpt/manifests/modules/objects.classes.selected.jsonl`
- Current counts: 143 PASS, 0 SKIP, 1992 FAIL, 0 BORK from 2136 corpus candidates

## Scope

- classes
- properties
- methods
- visibility
- magic
- traits
- enums

## Non-Scope

- Reflection API completion

## Relevant PHPT Paths

- `tests/lang/compare_objects_basic2.phpt`
- `tests/classes/visibility_005.phpt`
- `tests/classes/visibility_003b.phpt`
- `tests/classes/visibility_002b.phpt`
- `tests/classes/visibility_002a.phpt`
- `tests/classes/visibility_001b.phpt`
- `tests/classes/visibility_001a.phpt`
- `tests/classes/visibility_000b.phpt`
- `tests/classes/visibility_000a.phpt`
- `tests/classes/unset_properties.phpt`
- `tests/classes/type_hinting_005d.phpt`
- `tests/classes/type_hinting_005c.phpt`
- `tests/classes/type_hinting_005a.phpt`
- `tests/classes/type_hinting_004.phpt`
- `tests/classes/type_hinting_003.phpt`
- `tests/classes/type_hinting_002.phpt`
- `tests/classes/type_hinting_001.phpt`
- `tests/classes/tostring_004.phpt`
- `tests/classes/tostring_003.phpt`
- `tests/classes/tostring_002.phpt`
- `tests/classes/tostring_001.phpt`
- `tests/classes/this.phpt`
- `tests/classes/static_this.phpt`
- `tests/classes/static_properties_undeclared_read.phpt`
- `tests/classes/static_properties_undeclared_isset.phpt`
- `tests/classes/static_properties_undeclared_inc.phpt`
- `tests/classes/static_properties_undeclared_assignRef.phpt`
- `tests/classes/static_properties_undeclared_assignInc.phpt`
- `tests/classes/static_properties_undeclared_assign.phpt`
- `tests/classes/static_properties_004.phpt`
- `tests/classes/static_properties_003_error4.phpt`
- `tests/classes/static_properties_003_error3.phpt`
- `tests/classes/static_properties_003_error2.phpt`
- `tests/classes/static_properties_003_error1.phpt`
- `tests/classes/static_properties_003.phpt`
- `tests/classes/static_properties_001.phpt`
- `tests/classes/static_mix_2.phpt`
- `tests/classes/static_mix_1.phpt`
- `tests/classes/singleton_001.phpt`
- `tests/classes/serialize_001.phpt`

## Relevant php-src Source Areas

- `crates/php_semantics/`
- `crates/php_runtime/`
- `crates/php_vm/`

## Target Gates

- `nix develop -c just phpt-module MODULE=zend.objects`

## Known Gaps

- `runtime-error-or-diagnostic`: 983
- `runtime-unsupported-feature`: 620
- `runtime-output-mismatch`: 394
- `frontend-parse-or-compile`: 2
- `runtime-timeout`: 1

## Next Step

Stabilize constructor/property/method basics before magic behavior.
