# reflection

- Priority: 21
- Selected manifest: `tests/phpt/manifests/modules/reflection.selected.jsonl`
- Current counts: 10 PASS, 0 SKIP, 294 FAIL, 0 BORK from 304 corpus candidates

## Scope

- Reflection metadata for functions, classes, methods, properties, attributes

## Non-Scope

- fake metadata not backed by frontend/runtime/arginfo

## Relevant PHPT Paths

- `ext/uri/tests/015.phpt`
- `ext/reflection/tests/types/union_types.phpt`
- `ext/reflection/tests/types/pure_intersection_type_implicitly_nullable.phpt`
- `ext/reflection/tests/types/mixed_type.phpt`
- `ext/reflection/tests/types/intersection_types.phpt`
- `ext/reflection/tests/types/dnf_types_with_null.phpt`
- `ext/reflection/tests/types/dnf_types.phpt`
- `ext/reflection/tests/types/bug80190.phpt`
- `ext/reflection/tests/types/ReflectionType_002.phpt`
- `ext/reflection/tests/types/ReflectionType_001.phpt`
- `ext/reflection/tests/static_type.phpt`
- `ext/reflection/tests/static_properties_002.phpt`
- `ext/reflection/tests/request38992.phpt`
- `ext/reflection/tests/readonly_properties.phpt`
- `ext/reflection/tests/property_hooks/hook_guard.phpt`
- `ext/reflection/tests/property_hooks/gh17713.phpt`
- `ext/reflection/tests/property_hooks/gh15718.phpt`
- `ext/reflection/tests/property_hooks/bug_001.phpt`
- `ext/reflection/tests/property_hooks/basics.phpt`
- `ext/reflection/tests/property_hooks/ReflectionProperty_isInitialized.phpt`
- `ext/reflection/tests/property_hooks/ReflectionProperty_getSetValue.phpt`
- `ext/reflection/tests/property_hooks/ReflectionProperty_getSetRawValue.phpt`
- `ext/reflection/tests/property_hooks/ReflectionProperty_getHooks.phpt`
- `ext/reflection/tests/property_hooks/ReflectionProperty_getHook_inheritance.phpt`
- `ext/reflection/tests/property_exists.phpt`
- `ext/reflection/tests/parameters_002.phpt`
- `ext/reflection/tests/parameters_001.phpt`
- `ext/reflection/tests/new_in_constexpr.phpt`
- `ext/reflection/tests/new_in_attributes.phpt`
- `ext/reflection/tests/iterable_Reflection.phpt`
- `ext/reflection/tests/internal_static_property.phpt`
- `ext/reflection/tests/internal_property_union_type.phpt`
- `ext/reflection/tests/internal_parameter_default_value/check_all.phpt`
- `ext/reflection/tests/internal_parameter_default_value/ReflectionParameter_toString_Internal.phpt`
- `ext/reflection/tests/internal_parameter_default_value/ReflectionParameter_isDefaultValueConstant_Internal.phpt`
- `ext/reflection/tests/internal_parameter_default_value/ReflectionParameter_isDefaultValueAvailable_Internal.phpt`
- `ext/reflection/tests/internal_parameter_default_value/ReflectionParameter_getDefaultValue_Internal.phpt`
- `ext/reflection/tests/internal_parameter_default_value/ReflectionParameter_getDefaultValueConstantName_Internal.phpt`
- `ext/reflection/tests/gh9470.phpt`
- `ext/reflection/tests/gh9447.phpt`

## Relevant php-src Source Areas

- `ext/reflection/tests/`

## Target Gates

- `nix develop -c just phpt-module MODULE=reflection`

## Known Gaps

- `runtime-error-or-diagnostic`: 152
- `runtime-unsupported-feature`: 123
- `runtime-output-mismatch`: 19
- `runtime-timeout`: 1

## Next Step

Expose generated arginfo and semantic metadata through Reflection APIs.
