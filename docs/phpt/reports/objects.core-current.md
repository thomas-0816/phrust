# objects.core Current Focus Report

Generated from:

- `CARGO_TARGET_DIR="$PWD/target/b3-objects-core" PHPT_WORK_DIR="$PWD/target/phpt-work-b3-objects-core" REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just phpt-dev-build`
- `CARGO_TARGET_DIR="$PWD/target/b3-objects-core" PHPT_WORK_DIR="$PWD/target/phpt-work-b3-objects-core" REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=objects.core`

Current focused selected run:

| Outcome | Reference | Target |
| --- | ---: | ---: |
| PASS | 16 | 16 |
| FAIL | 0 | 0 |
| SKIP | 0 | 0 |
| BORK | 0 | 0 |

Initial target measurement before the `$this` routing fix was 15 PASS / 1 FAIL.
The single failure was `this-outside-context.phpt`, where the VM emitted
`E_PHP_VM_THIS_OUTSIDE_METHOD` as an uncaught internal runtime diagnostic
instead of PHP's catchable `Error`.

## Selected PHPTs

| PHPT | Coverage |
| --- | --- |
| `tests/phpt/generated/objects.core/object-new-basic.phpt` | `new C` object allocation |
| `tests/phpt/generated/objects.core/constructor-property.phpt` | constructor argument assignment |
| `tests/phpt/generated/objects.core/property-read-write.phpt` | public property read/write |
| `tests/phpt/generated/objects.core/method-call.phpt` | public method call and return value |
| `tests/phpt/generated/objects.core/this-inside-method.phpt` | `$this` binding inside a method |
| `tests/phpt/generated/objects.core/this-outside-context.phpt` | invalid `$this` outside method |
| `tests/phpt/generated/objects.core/private-property-external-error.phpt` | private property external read/write |
| `tests/phpt/generated/objects.core/protected-property-external-error.phpt` | protected property external read/write |
| `tests/phpt/generated/objects.core/private-method-external-error.phpt` | private method external call |
| `tests/phpt/generated/objects.core/protected-method-external-error.phpt` | protected method external call |
| `tests/phpt/generated/objects.core/catch-error-visibility-error.phpt` | `catch (Error)` for visibility failure |
| `tests/phpt/generated/objects.core/public-static-method.phpt` | public static method, `self::`, `parent::` |
| `tests/phpt/generated/objects.core/static-property-read-write.phpt` | public static property read/write, `self::$prop` |
| `tests/phpt/generated/objects.core/typed-property-default.phpt` | typed property default value |
| `tests/phpt/generated/objects.core/typed-property-uninitialized.phpt` | uninitialized typed property read |
| `tests/phpt/generated/objects.core/property-type-mismatch.phpt` | property type mismatch `TypeError` |

## Implemented Behavior

- Plain construction, constructor invocation, public properties, public methods,
  method return values, and `$this` method binding pass in the focused gate.
- Private/protected property reads and writes, private/protected method calls,
  and selected `catch (Error)` visibility routing pass.
- Public static methods, public static property reads/writes, `self::`, and
  `parent::` pass for the selected core contracts.
- Typed property defaults, uninitialized typed property reads, and property type
  mismatches pass for the selected core contracts.
- `$this` outside object context now routes through PHP `Error` with the
  reference-visible message `Using $this when not in object context`.

## Remaining Gaps

The `objects.core` close gate is green. Broader `objects.classes` still owns
advanced object gaps outside this branch, including magic methods, clone and
clone-with, traits, enums, property hooks, full inheritance visibility matrices,
and Reflection/SPL surfaces.

## objects.classes Impact

Closeout run:

- Command: `CARGO_TARGET_DIR="$PWD/target/b3-objects-core" PHPT_WORK_DIR="$PWD/target/phpt-work-b3-objects-core" REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=objects.classes`
- Reference: 200 PASS, 0 FAIL, 0 SKIP, 0 BORK
- Target: 164 PASS, 36 FAIL, 0 SKIP, 0 BORK

The remaining selected `objects.classes` failures are not in the object-core
close gate. They group into:

- autoload and ReflectionException catch-type behavior
- iterator/destructor ordering and exception behavior
- serialization, `__sleep`, and `__toString` object formatting
- class constant inheritance and dynamic class/constant lookup edge cases
- property-reference and by-reference static-property assignment gaps
- static-as-instance edge cases and broader object/reference COW behavior

These areas should be handed to `phpt/b3-objects-advanced` rather than expanded
inside this core branch.
