# Closure Core Runtime Current Report

This report tracks the closure branch cross-module runtime dashboard for
PHP 8.5.7 behavior. The oracle is
`/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php`.

## Prompt 1.1 Dashboard

Fresh focused module measurements from this branch:

| Module | Reference | Target | Status |
| --- | ---: | ---: | --- |
| `zend.basic` | 10 PASS | 10 PASS | green |
| `operators.conversions` | 5 PASS | 5 PASS | green |
| `arrays.references` | 6 PASS | 6 PASS | green |
| `zend.functions` | 29 PASS | 29 PASS | green |
| `objects.classes` | 200 PASS | 197 PASS / 3 FAIL | backlog |
| `closure.core` | 33 PASS | 33 PASS | green |

The new `closure.core` dashboard selects 33 focused/generated fixtures across:

- `$GLOBALS` and dynamic globals
- array spread/unpack, key normalization, COW, and references
- foreach by-value and by-reference semantics
- include local scope and include path behavior
- advanced/default parameter constants, variadics, and by-reference sends
- dynamic class, method, static method, and property access
- late static binding captured inside a closure
- fatal/warning output parity contracts

## Selected Fixtures

- `tests/phpt/generated/closure.core/dynamic-globals-alias.phpt`
- `tests/phpt/generated/zend.basic/smoke-array_self_add_globals-03f80836cf16.phpt`
- `tests/phpt/generated/zend.basic/smoke-variable_with_integer_name-48644f4034d7.phpt`
- `tests/phpt/generated/wp.core-language/array-spread-unpack.phpt`
- `tests/phpt/generated/wp.core-language/argument-unpacking.phpt`
- `tests/phpt/generated/arrays.references/core-key-normalization-append-unset.phpt`
- `tests/phpt/generated/arrays.references/core-cow-separation-on-write.phpt`
- `tests/phpt/generated/arrays.references/core-array-element-references.phpt`
- `tests/phpt/generated/arrays.references/core-foreach-by-value-snapshot.phpt`
- `tests/phpt/generated/arrays.references/core-foreach-by-reference-local.phpt`
- `tests/phpt/generated/filesystem.streams/include-local-semantics.phpt`
- `tests/phpt/generated/filesystem.streams/include-path-scope.phpt`
- `tests/phpt/generated/zend.functions/user-defaults-and-by-value.phpt`
- `tests/phpt/generated/zend.functions/default-constant-expression-params.phpt`
- `tests/phpt/generated/zend.functions/variadic-packing.phpt`
- `tests/phpt/generated/zend.functions/by-ref-local.phpt`
- `tests/phpt/generated/zend.functions/by-ref-array-element.phpt`
- `tests/phpt/generated/zend.functions/by-ref-mismatch.phpt`
- `tests/phpt/generated/zend.functions/dynamic-first-class-callables.phpt`
- `tests/phpt/generated/wp.core-language/dynamic-class-instantiation.phpt`
- `tests/phpt/generated/wp.core-language/dynamic-method-call.phpt`
- `tests/phpt/generated/wp.core-language/dynamic-static-method-call.phpt`
- `tests/phpt/generated/wp.core-language/dynamic-property-access.phpt`
- `tests/phpt/generated/wp.core-language/dynamic-property-negative.phpt`
- `tests/phpt/generated/closure.core/late-static-closure-binding.phpt`
- `tests/phpt/generated/objects.core/public-static-method.phpt`
- `tests/phpt/generated/objects.core/static-property-read-write.phpt`
- `tests/phpt/generated/diagnostics.output/include-missing-warning.phpt`
- `tests/phpt/generated/diagnostics.output/undefined-variable-warning.phpt`
- `tests/phpt/generated/diagnostics.output/array-to-string-warning.phpt`
- `tests/phpt/generated/diagnostics.output/builtin-arity-error.phpt`
- `tests/phpt/generated/diagnostics.output/builtin-type-error.phpt`
- `tests/phpt/generated/diagnostics.output/invalid-operand-type-error.phpt`

## Top Failing IDs And Owner Layer

The fresh `objects.classes` target run is the only non-green Prompt 1.1 source
module. Its top failure groups are:

| Failure group | Representative IDs | Owner layer |
| --- | --- | --- |
| remaining fatal/exception output parity | class-not-found and iterator exception cases | `php_vm` and `php_runtime::error_output` |
| unsupported by-reference static/property targets | `E_PHP_IR_UNSUPPORTED_HIR_STATEMENT`, `E_PHP_IR_UNSUPPORTED_PROPERTY_REFERENCE` | `php_ir`, `php_runtime::reference`, `php_runtime::object` |
| iterator/destructor order and exception routing | iterator and destructor class fixtures | `php_vm`, `php_runtime::object` |
| stringification and object formatting parity | `tostring_001` | `php_runtime::object`, standard object formatting |
| class constants, dynamic lookup, autoload, and Reflection catch types | eval declaration, autoload, ReflectionException fixtures | `php_semantics`, `php_runtime::object`, `php_vm` |

These are dashboard gaps, not selected `closure.core` gate failures. The
closure branch will close only the vertical runtime semantics named by the
prompt pack and will keep broader object/reflection/SPL behavior documented as
known gaps unless a selected fixture requires it.

## Verification

Prompt 1.1 focused source gates already run on this branch:

- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.basic`: PASS, reference 10 PASS and target 10 PASS.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=operators.conversions`: PASS, reference 5 PASS and target 5 PASS.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=arrays.references`: PASS, reference 6 PASS and target 6 PASS.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.functions`: PASS, reference 29 PASS and target 29 PASS.
- `TMPDIR=/Volumes/CrucialMusic/tmp/phrust-phpt-objects-serializable-final REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_WORK_DIR=/Volumes/CrucialMusic/tmp/phrust-phpt-objects-serializable-final PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=objects.classes`: FAIL as expected for the dashboard, reference 200 PASS and target 197 PASS / 3 FAIL. `tests/classes/autoload_020.phpt`, `tests/classes/bug65768.phpt`, and `tests/classes/serialize_001.phpt` are now PASS.

Prompt 1.1 closeout:

- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=closure.core`: PASS, reference 33 PASS and target 33 PASS.

Prompt 1.1 broad check:

- `PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php nix develop -c just verify-phpt`: PASS, known-gap manifest validated, baseline verified, source integrity verified 24475 entries with 0 skipped, and `php_phpt_tools` tests passed.

## Runtime Fixes

- Dense instance method calls now dispatch `Fiber` and `Generator` receiver
  values through the same runtime method helpers as the rich interpreter path.
  This keeps `valid()`, `send()`, `getReturn()`, and related generator/fiber
  methods from being misclassified as non-object calls in dense execution.
- Dense and callable static method calls now route enum pseudo-static methods
  `cases`, `from`, and `tryFrom` through enum runtime handling before ordinary
  method lookup. This fixes unit enum case enumeration through static calls.
- The PHP-compatible `phrust-php` executor now suppresses structured runtime
  diagnostics on stderr when stdout already contains a PHP-rendered fatal or
  parse error. The developer `php-vm run` path keeps structured diagnostics.
- Runtime hardening clippy now passes after reducing duplicated literal
  lowering branches and collapsing small control-flow helper patterns in the
  semantic/IR lowering layers.
- Runtime class materialization now preserves class-constant initializer spans
  when initializer evaluation throws. PHP-compatible fatal output for selected
  initializer failures now reports the declaration line and a synthetic
  `[constant expression]()` stack frame, matching `constants_error_004.phpt`.
- Nested `print_r()` object property arrays now use PHP-compatible multiline
  indentation, matching `bug23951.phpt`.
- Runtime class declarations inside `try` blocks now validate missing
  parent/interface dependencies through the active exception handler, and failed
  declarations are hidden from subsequent `class_exists()` checks. This matches
  the selected `bug75765.phpt` catchability path.
- The runtime date builtin surface now includes `date_diff()` over the existing
  DateTimeInterface object payload. `bug65768.phpt` now matches the interval
  values and DateTimeInterface fatal line without a synthetic stack trace.
- `serialize()` now invokes public instance `__sleep()` methods in the VM before
  object serialization, preserves selected private/protected/public properties,
  recognizes PHP's mangled parent-private property names, and emits the
  PHP-compatible missing-property warning while continuing. This matches the
  selected `bug26737.phpt` and `private_members_serialization.phpt` rows.
- `unserialize()` now runs through a VM-owned class resolution pass so serialized
  object names trigger registered autoload callbacks. Unresolved classes are
  materialized as `__PHP_Incomplete_Class` with
  `__PHP_Incomplete_Class_Name`, matching the selected `autoload_020.phpt` row.
- Legacy `Serializable` objects now emit the PHP 8.5 deprecation, route
  `serialize()` through public instance `Serializable::serialize()`, encode the
  legacy `C:<class>:{payload}` wire format, call `Serializable::unserialize()`
  during `unserialize()`, and throw the selected catchable `Exception` when
  `serialize()` returns a non-string/non-NULL value. This matches the selected
  `serialize_001.phpt` row.
- Braced property interpolation now lowers `{$this->property}` through the same
  property-fetch path as `$this->property`, which keeps selected magic and
  serialization method output byte-compatible with PHP.

## Closed And Narrowed Gap IDs

The closure branch keeps these rows as `known_gap` when broader PHP matrices
remain, but the current behavior and executable fixtures are now narrowed:

| Gap ID | Change |
| --- | --- |
| `E_PHP_RUNTIME_SUPERGLOBALS_FULL_MATRIX` | Added selected dynamic `$GLOBALS[$name]` evidence while leaving SAPI population out of scope. |
| `E_PHP_RUNTIME_GLOBALS_ALIAS_MATRIX` | Added selected direct and dynamic `$GLOBALS` alias coverage; append/proxy and unset/reference alias edges remain gaps. |
| `E_PHP_IR_UNSUPPORTED_ARRAY_SPREAD` | Replaced the stale lowering-diagnostic-only note with `ArraySpread` IR and VM merge coverage for selected array operands. |
| `E_PHP_RUNTIME_VARIADIC_PACKED_ARRAY_ONLY` | Replaced the packed-list-facade note with ordinary `PhpArray` variadic packing for selected positional and named tails. |
| `E_PHP_IR_UNSUPPORTED_METHOD_CALL` | Added selected dynamic instance/static method and nullsafe method execution evidence. |
| `E_PHP_IR_UNSUPPORTED_HIR_STATEMENT` | Added selected dynamic class instantiation, dynamic property, and nullsafe property execution evidence. |
| `E_PHP_IR_UNSUPPORTED_LATE_STATIC_BINDING` | Added selected `self::`, `parent::`, and `static::class` closure-binding evidence. |
| `E_PHP_RUNTIME_UNSUPPORTED_THROWABLE_HIERARCHY` | Narrowed PHP-compatible CLI fatal output by removing duplicate structured stderr when a fatal line is rendered and by rendering class-constant initializer failures at the initializer line with a `[constant expression]()` trace frame; full stack/object parity remains open. |
| `E_PHP_VM_TOO_FEW_ARGS`, `E_PHP_VM_TOO_MANY_ARGS` | Narrowed diagnostics to catchable selected arity objects while leaving exact engine wording and stack formatting open. |
| `E_PHP_RUNTIME_BUILTIN_ARITY`, `E_PHP_RUNTIME_BUILTIN_TYPE` | Added selected catchable builtin arity/type diagnostic fixtures; full arginfo/coercion matrices remain gaps. |

The highest-risk remaining core blockers are iterator/destructor ordering,
eval declaration merging, Reflection catch behavior, and remaining
fatal text/object-id parity. Those blockers explain the current
`objects.classes` dashboard backlog rather than the selected `closure.core`
gate.

## Prompt 1.2-1.6 Verification

The focused implementation gates were rerun after the runtime fixes:

- Prompt 1.2 globals: `cargo test -p php_runtime globals` PASS (3 tests), `cargo test -p php_vm` PASS (492 tests), `just phpt-dev-module MODULE=closure.core` PASS (33/33), and `just runtime-semantics-fixtures` PASS with refs/COW 7 pass + 2 known gaps, object semantics 73 pass + 10 known gaps, generator/fiber 26 pass + 1 known gap, real-world 1 pass + 2 known gaps, regressions 2 pass + 1 known gap.
- Prompt 1.3 arrays: `cargo test -p php_runtime array` PASS (61 tests), `cargo test -p php_ir` PASS (69 unit tests + 7 bytecode snapshots), `cargo test -p php_vm` PASS (492 tests), `just phpt-dev-module MODULE=closure.core` PASS (33/33), and `just phpt-dev-module MODULE=standard.arrays` PASS (17/17).
- Prompt 1.4 references: `cargo test -p php_runtime reference` PASS (22 tests), `cargo test -p php_runtime array` PASS (61 tests), `cargo test -p php_vm` PASS (492 tests), `just phpt-dev-module MODULE=arrays.references` PASS (6/6), and `just phpt-dev-module MODULE=closure.core` PASS (33/33).
- Prompt 1.5 dynamic objects/static binding: `cargo test -p php_ir` PASS (69 unit tests + 7 bytecode snapshots), `cargo test -p php_runtime object` PASS (30 tests), `cargo test -p php_vm` PASS (493 tests), `just phpt-dev-module MODULE=closure.core` PASS (33/33), and `just phpt-dev-module MODULE=objects.classes` remains dashboard backlog with reference 200 PASS and target 197 PASS / 3 non-green.
- Prompt 1.6 diagnostics/output: `cargo test -p php_runtime` PASS (256 tests), `cargo test -p php_vm` PASS (492 tests), `just phpt-dev-module MODULE=diagnostics.output` PASS (6/6), and `just phpt-dev-module MODULE=closure.core` PASS (33/33).

All PHPT module runs above used:

`REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0`

Each PHPT module run verified 24475 php-src manifest entries with 0 skipped
host-generated entries.

## Prompt 1.7 Closeout

- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just verify-runtime`: PASS. Runtime fixtures passed 125 cases; runtime semantics diff passed with total 289, pass 240, fail 0, skip 0, known_gap 49; runtime hardening clippy passed.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just verify-stdlib`: PASS. `php_std` passed 66 tests plus registry dump, VM std bridge passed 2 tests, standard-library diff totals were stdlib 37 pass + 6 known gaps, streams 2 pass, json/pcre/date 3 pass, and spl/reflection 2 pass.
- Closeout module dashboard: `closure.core` 33/33 PASS, `zend.basic` 10/10 PASS, `operators.conversions` 5/5 PASS, `arrays.references` 6/6 PASS, `zend.functions` 29/29 PASS, and `objects.classes` remains 197 PASS / 3 non-green against a 200 PASS reference run.
- Known-gap catalog updates: `docs/runtime-known-gaps.md` and `docs/known_gaps/runtime.jsonl` now record the closed/narrowed closure-core rows above.
