# objects.classes

- Priority: 10
- Selected manifest: `tests/phpt/manifests/modules/objects.classes.selected.jsonl`
- Current corpus counts: 178 PASS, 33 SKIP, 1924 FAIL, 0 BORK from 2136 corpus candidates
- Current selected run: 164 PASS, 0 SKIP, 36 FAIL, 0 BORK from 200 selected rows
- Core close gate: `objects.core` is 16 PASS / 0 FAIL for reference and target

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
- `nix develop -c just phpt-dev-module MODULE=objects.core`
- `nix develop -c just phpt-dev-module MODULE=objects.classes`

## Known Gaps

- `runtime-error-or-diagnostic`: 983
- `runtime-unsupported-feature`: 620
- `runtime-output-mismatch`: 394
- `frontend-parse-or-compile`: 2
- `runtime-timeout`: 1

Current selected `objects.classes` non-green rows after the `objects.core`
branch are outside the core construction/property/method/visibility/static/type
slice. The remaining selected failures group around:

- autoload and ReflectionException catch-type behavior
- iterator/destructor ordering and exception behavior
- serialization, `__sleep`, and `__toString` object formatting
- class constant inheritance and dynamic constant/class lookup edge cases
- property-reference and by-reference static-property assignment gaps
- static-as-instance edge cases and broader object/reference COW behavior

## Next Step

Hand off the remaining advanced object subareas to `phpt/b3-objects-advanced`.
Keep `objects.core` as the branch-local close gate for construction,
constructors, public properties, public methods, `$this`, selected
private/protected visibility errors, public static access, and typed-property
basics.
