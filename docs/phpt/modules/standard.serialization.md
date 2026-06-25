# standard.serialization

- Priority: 16
- Selected manifest: `tests/phpt/manifests/modules/standard.serialization.selected.jsonl`
- Current counts: 16 PASS, 2 SKIP, 107 FAIL, 0 BORK from 126 corpus candidates

## Scope

- serialize
- unserialize
- value persistence

## Non-Scope

- session module persistence

## Relevant PHPT Paths

- `ext/standard/tests/serialize/unserialize_uppercase_s.phpt`
- `ext/standard/tests/serialize/unserialize_ref_to_overwritten_declared_prop.phpt`
- `ext/standard/tests/serialize/unserialize_overwrite_undeclared_protected.phpt`
- `ext/standard/tests/serialize/unserialize_mem_leak.phpt`
- `ext/standard/tests/serialize/unserialize_leak.phpt`
- `ext/standard/tests/serialize/unserialize_large.phpt`
- `ext/standard/tests/serialize/unserialize_extra_data_003.phpt`
- `ext/standard/tests/serialize/unserialize_extra_data_002.phpt`
- `ext/standard/tests/serialize/unserialize_extra_data_001.phpt`
- `ext/standard/tests/serialize/unserializeS.phpt`
- `ext/standard/tests/serialize/typed_property_refs.phpt`
- `ext/standard/tests/serialize/typed_property_ref_overwrite2.phpt`
- `ext/standard/tests/serialize/typed_property_ref_overwrite.phpt`
- `ext/standard/tests/serialize/typed_property_ref_assignment_failure.phpt`
- `ext/standard/tests/serialize/sleep_uninitialized_typed_prop.phpt`
- `ext/standard/tests/serialize/sleep_undefined_declared_properties.phpt`
- `ext/standard/tests/serialize/sleep_mangled_name_clash.phpt`
- `ext/standard/tests/serialize/sleep_deref.phpt`
- `ext/standard/tests/serialize/shm_corruption_coercion_unserialize_options.phpt`
- `ext/standard/tests/serialize/serialize_globals_var_refs.phpt`
- `ext/standard/tests/serialize/serialization_resources_001.phpt`
- `ext/standard/tests/serialize/serialization_precision_002.phpt`
- `ext/standard/tests/serialize/serialization_miscTypes_001.phpt`
- `ext/standard/tests/serialize/serialization_error_002.phpt`
- `ext/standard/tests/serialize/ref_to_failed_serialize.phpt`
- `ext/standard/tests/serialize/precision.phpt`
- `ext/standard/tests/serialize/overwrite_untyped_ref.phpt`
- `ext/standard/tests/serialize/oss_fuzz_433303828.phpt`
- `ext/standard/tests/serialize/max_depth.phpt`
- `ext/standard/tests/serialize/invalid_signs_in_lengths.phpt`
- `ext/standard/tests/serialize/gh19701.phpt`
- `ext/standard/tests/serialize/gh15169.phpt`
- `ext/standard/tests/serialize/gh12265b.phpt`
- `ext/standard/tests/serialize/gh12265.phpt`
- `ext/standard/tests/serialize/bug81163.phpt`
- `ext/standard/tests/serialize/bug81142.phpt`
- `ext/standard/tests/serialize/bug81111.phpt`
- `ext/standard/tests/serialize/bug80411.phpt`
- `ext/standard/tests/serialize/bug79526.phpt`
- `ext/standard/tests/serialize/bug78438.phpt`

## Relevant php-src Source Areas

- `ext/standard/tests/serialize/`

## Target Gates

- `nix develop -c just phpt-module MODULE=standard.serialization`

## Known Gaps

- `runtime-output-mismatch`: 55
- `runtime-error-or-diagnostic`: 35
- `runtime-unsupported-feature`: 25

## Next Step

Implement serialization after arrays/objects are stable.
