# objects.classes

- Priority: 10
- Selected manifest: `tests/phpt/manifests/modules/objects.classes.selected.jsonl`
- Current corpus counts: 178 PASS, 33 SKIP, 1924 FAIL, 0 BORK from 2136 corpus candidates
- Current selected run: 183 PASS, 0 SKIP, 63 FAIL, 0 BORK from 246 selected rows
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

## Branch 1 Closure Runtime Impact

On `phpt/closure-core-runtime-semantics`, after the closure runtime semantics
work plus selected class-output/declaration dependency/serialization/autoload
fixes:

- `TMPDIR=/Volumes/CrucialMusic/tmp/phrust-phpt-objects-serializable-final REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_WORK_DIR=/Volumes/CrucialMusic/tmp/phrust-phpt-objects-serializable-final PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=objects.classes`
- reference: 200 PASS
- target: 197 PASS, 3 FAIL

The selected `tests/classes/constants_error_004.phpt` case now matches PHP's
class-constant initializer fatal location and `[constant expression]()` trace
frame. The selected `tests/classes/bug23951.phpt` and
`tests/classes/bug75765.phpt` cases now match PHP for nested `print_r()` object
property arrays and catchable missing-parent class declarations. The selected
`tests/classes/bug65768.phpt` case now matches PHP's interval values and
DateTimeInterface fatal line after `date_diff()` without a synthetic stack trace
block. The selected `tests/classes/bug26737.phpt` and
`tests/classes/private_members_serialization.phpt` cases now match PHP for
`__sleep()` property selection, including missing-property warnings and mangled
parent-private property names.
The selected `tests/classes/autoload_020.phpt` case now triggers registered
autoload callbacks during `unserialize()` and materializes an unresolved class as
`__PHP_Incomplete_Class` with `__PHP_Incomplete_Class_Name`, matching PHP.
The selected `tests/classes/serialize_001.phpt` case now matches PHP for the
legacy `Serializable` deprecation, `Serializable::serialize()` and
`Serializable::unserialize()` hooks, the `C:<class>:{payload}` wire format,
NULL serialization, and the catchable non-string return exception path.

## Wave 4 Core Language Object Promotion

On `phpt/wave4-core-language-object-promotion`, the selected manifest expanded
from 200 to 246 upstream rows.

- Promoted rows: 46 upstream `tests/classes/*.phpt` cases.
- Current selected gate:
  `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=objects.classes`
- Reference: 246 PASS, 0 non-green.
- Target: 183 PASS, 63 FAIL.
- Source integrity: 24475 php-src manifest entries checked, 0 skipped.

The branch also made these already-selected upstream rows green:
`constants_basic_006.phpt`, `constants_basic_003.phpt`,
`static_properties_003.phpt`, `destructor_and_exceptions.phpt`, and
`bug26737.phpt`.

## Branch 2 Advanced Integration Impact

On `main`, after the completed object-core branch and the Branch 2 advanced
object fixtures are integrated, the earlier selected gate snapshot was:

- `nix develop -c just phpt-dev-module MODULE=objects.classes`
- reference: 200 PASS
- target: 164 PASS, 36 FAIL

The four advanced submodule gates are split into `objects.magic`,
`objects.clone`, `objects.traits`, and `objects.enums` and pass independently.
The remaining selected aggregate failures group around autoload and
ReflectionException catch-type behavior, iterator/destructor ordering and
exception behavior, serialization, `__sleep`, and `__toString` object
formatting, class constant inheritance and dynamic constant/class lookup edge
cases, property-reference and by-reference static-property assignment gaps,
static-as-instance edge cases, and broader object/reference COW behavior.

## Known Gaps

- `runtime-error-or-diagnostic`: 983
- `runtime-unsupported-feature`: 620
- `runtime-output-mismatch`: 394
- `frontend-parse-or-compile`: 2
- `runtime-timeout`: 1

Current selected `objects.classes` non-green rows are outside the
`wp.core-language` slice. The remaining selected failures group around:

- ReflectionException catch-type behavior
- iterator/destructor ordering and exception behavior
- `__toString` object formatting and object-id parity
- eval declaration merging
- object formatting parity

## Next Step

Keep `objects.core` as the construction/property/method/visibility/static/type
regression gate. Continue reducing the remaining selected `objects.classes`
failures by owned runtime area while keeping the advanced submodule gates green.
