# Wave 4 Core Language Object Promotion Current State

## Scope

- Branch: `phpt/wave4-core-language-object-promotion`
- Base when branch was created: `6a1b2ea`
- Prompt prepared base: `cdec4c7`
- Reference target: PHP `8.5.7`
- Reference binary used: `/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php`
- PHP source tree used for integrity checks: `/Volumes/CrucialMusic/src/phrust/third_party/php-src`

The branch-local `third_party/php-src/sapi/cli/php` binary is not available in
this checkout, so the sibling pinned php-src oracle above was used for all
comparison runs in this report.

## Inventory Commands

All inventory runs used:

```bash
REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php \
PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src \
PHPT_REUSE_LAST=0 \
PHPT_DEV_REUSE_TARGET_PASS=0 \
nix develop -c just phpt-dev-module MODULE=<module>
```

| Module | Reference | Target | Source integrity | Result |
| --- | ---: | ---: | ---: | --- |
| `objects.classes` | 246 pass, 0 non-green | 246 pass, 0 non-green | 24475 checked, 0 skipped | green |
| `zend.functions` | 29 pass, 0 non-green | 29 pass, 0 non-green | 24475 checked, 0 skipped | green |
| `zend.basic` | 10 pass, 0 non-green | 10 pass, 0 non-green | 24475 checked, 0 skipped | green |

The selected manifest at branch start had 200 rows. This branch promoted 46
additional upstream `tests/classes` PHPTs that were reference-clean and
target-clean, bringing the selected manifest to 246 rows. Prompt 1.2-1.5 work
also moved five already-selected upstream tests from failing to passing:
`constants_basic_006.phpt`, `constants_basic_003.phpt`,
`static_properties_003.phpt`, `destructor_and_exceptions.phpt`, and
`bug26737.phpt`.

## Promoted PHPTs

The following initial upstream php-src tests were promoted into
`tests/phpt/manifests/modules/objects.classes.selected.jsonl`:

- `tests/classes/__call_001.phpt`
- `tests/classes/__call_003.phpt`
- `tests/classes/__call_004.phpt`
- `tests/classes/__set__get_001.phpt`
- `tests/classes/__set__get_004.phpt`
- `tests/classes/__set__get_005.phpt`
- `tests/classes/class_example.phpt`
- `tests/classes/clone_001.phpt`
- `tests/classes/clone_002.phpt`
- `tests/classes/clone_004.phpt`
- `tests/classes/factory_001.phpt`
- `tests/classes/final.phpt`
- `tests/classes/inheritance.phpt`
- `tests/classes/object_reference_001.phpt`
- `tests/classes/visibility_003a.phpt`

Initial promotion probe:

- 20 nearby unselected upstream class tests were measured with a temporary
  manifest.
- 15 passed target and were promoted.
- 5 remained blocked and were not promoted in Prompt 1.1:
  `__call_002.phpt`, `__call_005.phpt`, `__call_006.phpt`,
  `__set__get_002.phpt`, and `__set__get_003.phpt`.

The following closeout upstream php-src tests were also promoted after probing
all remaining unselected `tests/classes/*.phpt` rows:

- `tests/classes/__set_data_corrupt.phpt`
- `tests/classes/abstract_final.phpt`
- `tests/classes/array_conversion_keys.phpt`
- `tests/classes/assign_op_property_001.phpt`
- `tests/classes/autoload_001.phpt`
- `tests/classes/bug24399.phpt`
- `tests/classes/bug24445.phpt`
- `tests/classes/constants_scope_001.phpt`
- `tests/classes/constants_visibility_001.phpt`
- `tests/classes/constants_visibility_008.phpt`
- `tests/classes/destructor_and_references.phpt`
- `tests/classes/destructor_visibility_003.phpt`
- `tests/classes/inheritance_008.phpt`
- `tests/classes/interface_constant_inheritance_001.phpt`
- `tests/classes/interface_constant_inheritance_004.phpt`
- `tests/classes/interface_constant_inheritance_005.phpt`
- `tests/classes/interface_optional_arg.phpt`
- `tests/classes/interface_optional_arg_002.phpt`
- `tests/classes/interfaces_001.phpt`
- `tests/classes/private_006b.phpt`
- `tests/classes/property_override_privateStatic_privateStatic.phpt`
- `tests/classes/property_override_privateStatic_protectedStatic.phpt`
- `tests/classes/property_override_privateStatic_publicStatic.phpt`
- `tests/classes/property_override_private_private.phpt`
- `tests/classes/property_override_private_privateStatic.phpt`
- `tests/classes/property_override_protectedStatic_protectedStatic.phpt`
- `tests/classes/property_override_protectedStatic_publicStatic.phpt`
- `tests/classes/property_override_protected_public.phpt`
- `tests/classes/property_override_publicStatic_publicStatic.phpt`
- `tests/classes/property_override_public_public.phpt`
- `tests/classes/visibility_000c.phpt`

Closeout promotion probe:

- 69 unselected upstream `tests/classes` PHPTs were measured with a temporary
  comparison manifest.
- All 69 were reference-clean.
- 37 were target-clean after the object array-cast fix; 31 were promoted in this
  bounded closeout batch.
- The remaining target-clean rows are listed in the next-promotion backlog
  below so the next batch can stay reviewable.

## Failure Clusters

The current `objects.classes` selected target run has no non-green rows.

The final magic/lifetime/reflection fixes closed the last two selected
blockers:

- `tests/classes/autoload_021.phpt`: invalid path-like dynamic class names no
  longer trigger registered autoload callbacks before the unresolved-class
  fatal.
- `tests/classes/constants_error_003.phpt`: class constants passed to
  by-reference parameters keep PHP's catchable `Error` behavior and render the
  uncaught fatal with the caller-only trace.

## Prompt 1.2 Routing

Prompt 1.2 started with eval/autoload/declaration visibility because it has
the largest direct blocker cluster and unlocks later constant and reflection
cases. The initial concrete starting points were:

- `constants_basic_006.phpt`: dynamic eval declaration handling now routes
  through the request-local declaration tables; remaining failures should be
  classified under the narrower conditional-function or constant-redeclaration
  compatibility IDs when reproduced.
- `autoload_010.phpt` and `autoload_011.phpt`: missing autoload-triggered
  class/interface declaration behavior
- `autoload_016.phpt` and `autoload_017.phpt`: reflection/autoload interactions
- `autoload_018.phpt`: lowering gap around a non-simple increment/decrement
  expression in an autoload-heavy flow

Implemented in Prompt 1.2:

- `eval()` now registers named function, class, and constant declarations into
  request-local dynamic runtime tables instead of raising
  conditional-function declaration diagnostics where those narrower IDs match
  the reproduced behavior. PHP 8.5 duplicate global `const` warnings are
  covered for the selected include/eval redeclaration fixtures.
- Eval redeclaration checks now reject duplicate functions, classes, and
  constants before merging the dynamic unit.
- Runtime class, class-constant, and static-property hierarchy lookup can see
  eval/include dynamic classes while preserving the existing static lookup
  helpers for compile-time validation.
- Runtime semantic fixtures for eval-declared classes and functions moved from
  known gaps to expected passing fixtures.

Follow-up Prompt 1.2/1.4 fixes:

- `constants_basic_006.phpt` is now green. Property defaults preserve unresolved
  global and class constants symbolically in IR and evaluate them against the
  request state when static properties are read.
- `constants_basic_003.phpt` is now green. Class constant initializers preserve
  unresolved class constants symbolically and class constant fetch/cache reads
  evaluate them with request-local class/global constant lookup.

Prompt 1.3 fixes:

- `static_properties_003.phpt` is now green. Public static properties accessed
  through instance syntax emit PHP-compatible notices and use dynamic instance
  slots for unset, read, assignment, and reference assignment while preserving
  the real static property value.
- `destructor_and_exceptions.phpt` is now green. User classes can extend known
  internal throwable parents case-insensitively, and class relation checks treat
  those user subclasses as instances of the internal parent for catch matching.

Prompt 1.5 fixes:

- `bug26737.phpt` is now green. Object serialization now intercepts userland
  `__sleep()` for single-object `serialize()` calls, validates the returned
  property list, maps public/protected/private and mangled property names to VM
  storage keys, emits missing-property warnings, and serializes only the
  selected object properties.
- `private_members_serialization.phpt` was kept green while adding `__sleep()`
  selection, including parent-private properties returned with PHP's mangled
  `"\0Class\0name"` spelling.

Prompt 1.6 and closeout promotion fixes:

- The static-as-instance and reference-slot changes kept
  `static_properties_003.phpt` green and left by-reference static-property
  assignment gaps explicit in the remaining selected failures.
- The closeout batch promoted static-property redeclaration, property
  assign-op, destructor/reference, interface inheritance, visibility, and class
  constant PHPTs that already match reference output.
- Explicit object-to-array casts now emit PHP's mangled private and protected
  property keys, promoting `tests/classes/array_conversion_keys.phpt`.

## Acceptance Evidence

- `objects.classes` selected run after current Wave 4 object fixes:
  246 reference-clean rows, target 246 pass / 0 fail.
- `zend.functions` selected run: 29 reference pass / 29 target pass.
- `zend.basic` selected run: 10 reference pass / 10 target pass.
- `standard.serialization` selected run: 5 reference pass / 5 target pass.
- `standard.variables` selected run: 27 reference pass / 27 target pass.
- Source integrity during module runs: 24475 php-src manifest entries checked,
  0 host-generated entries skipped.
- Current `php_ir` cargo gate:
  `nix develop -c cargo test -p php_ir` passed with 91 unit tests,
  7 bytecode snapshot tests, and doc tests.
- Current `php_vm` cargo gate:
  `nix develop -c cargo test -p php_vm` passed with 492 unit tests and doc
  tests.
- Current `php_runtime` cargo gate:
  `nix develop -c cargo test -p php_runtime` passed with 262 unit tests and
  doc tests.
- Prompt 1.4 semantic gate:
  `nix develop -c cargo test -p php_semantics` passed with 93 unit tests and
  doc tests.
- Prompt 1.6 reference-focused runtime gate:
  `nix develop -c cargo test -p php_runtime reference` passed with 22 selected
  tests and 240 filtered tests.
- Aggregate runtime gate:
  `nix develop -c just verify-runtime` passed, including bytecode snapshots,
  runtime fixtures, runtime semantic diffs, and runtime hardening clippy.
- Aggregate stdlib gate:
  `nix develop -c just verify-stdlib` passed, including stdlib docs,
  coverage, and stdlib/streams/json-pcre-date/spl-reflection diffs.
- Aggregate PHPT tooling gate:
  `nix develop -c just verify-phpt` passed, including known-gap validation,
  PHPT baseline verification, php-src source integrity, and PHPT tool tests.
- Prompt 1.2 runtime semantic fixture gate:
  `nix develop -c just runtime-semantics-fixtures` passed.
- Focused Prompt 1.3/1.4 tests passed:
  `class_constants_resolve_runtime_class_constant_initializers`,
  `static_properties_bound_as_instance_slots_create_dynamic_reference`,
  `static_properties_accessed_as_instance_slots_emit_notices`, and
  `exceptions_allow_user_classes_to_extend_internal_exception_case_insensitively`.
- Focused Prompt 1.5 tests passed:
  `cargo test -p php_runtime serialization`,
  `cargo test -p php_vm serialize_uses_sleep_property_selection_and_warnings`,
  focused PHPT `tests/classes/bug26737.phpt`, and focused PHPT
  `tests/classes/private_members_serialization.phpt`.
- Focused upstream PHPTs now green:
  `constants_basic_003.phpt`, `constants_basic_006.phpt`,
  `static_properties_003.phpt`, `destructor_and_exceptions.phpt`, and
  `bug26737.phpt`.
- Additional focused object-formatting PHPT now green:
  `tests/classes/array_conversion_keys.phpt`.
- Additional magic/lifetime/reflection upstream PHPTs now green:
  `tests/classes/autoload_021.phpt` and
  `tests/classes/constants_error_003.phpt`.
- Current module gates:
  `objects.classes` is green at 246 reference pass / 246 target pass;
  `zend.functions`, `zend.basic`, `standard.serialization`, and
  `standard.variables` remain green for their selected manifests.

## End Report

- Promoted upstream PHPT count: 46.
- Selected manifest before this branch: 200 rows.
- Selected manifest after this branch: 246 rows.
- Current selected module count:
  `objects.classes` reference 246 pass / target 246 pass / 0 fail.
- Behavioral fixes moved these already-selected upstream rows to green:
  `constants_basic_006.phpt`, `constants_basic_003.phpt`,
  `static_properties_003.phpt`, `destructor_and_exceptions.phpt`,
  `bug26737.phpt`, `autoload_021.phpt`, and `constants_error_003.phpt`.

Remaining blocker clusters:

- None in the current selected `objects.classes` gate.

Next 10 upstream PHPTs to promote:

- `tests/classes/visibility_001c.phpt`
- `tests/classes/visibility_002c.phpt`
- `tests/classes/visibility_003c.phpt`
- `tests/classes/visibility_004a.phpt`
- `tests/classes/visibility_004b.phpt`
- `tests/classes/visibility_004c.phpt`
- `tests/classes/__call_002.phpt`
- `tests/classes/__set__get_002.phpt`
- `tests/classes/autoload_002.phpt`
- `tests/classes/__set__get_003.phpt`
