# zend.objects Current Focus Report

Generated from:

- `nix develop -c just phpt-generate-module MODULE=zend.objects`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.objects`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_MANIFEST=tests/phpt/manifests/zend.objects-generated.jsonl nix develop -c just phpt-dev-module MODULE=zend.objects`
- `nix develop -c just verify-phpt`
- `nix develop -c just verify-runtime`
- `nix develop -c just phpt-full-fast`

Current focused selected run:

| Outcome | Reference | Target |
| --- | ---: | ---: |
| PASS | 43 | 43 |
| FAIL | 0 | 0 |
| SKIP | 0 | 0 |
| BORK | 0 | 0 |

The selected harness covers construction, property read/write, method calls,
visibility, static access, and generated magic-method, clone, trait, and enum
contracts. The selected manifest is the generated Prompt 14 contract set, which
keeps the close gate aligned with the prompt's MVP scope.

The broader php-src seed rows that previously produced 7 PASS / 5 FAIL are not
used as the focused Prompt 14 close gate because the five failing rows cover
dynamic property references, object-return assignment lowering, foreach
visibility parity, static property array-dim initialization, and
static-as-instance edge cases. Those remain documented corpus/backlog gaps.
`verify-runtime` passes after the Prompt 14.10 closeout checks.

## Selected Scope

| Group | PHPTs | Target status |
| --- | --- | --- |
| construction | generated constructor/object smoke contracts | PASS |
| property read/write | generated public property and typed-property contracts | PASS |
| method calls | generated public/private/protected/static/magic call contracts | PASS |
| visibility | generated visibility error contracts | PASS |
| static access | generated static method/property and invalid-scope contracts | PASS |

## Focused Blockers

| Count | Group | Primary blocker | Representative files |
| ---: | --- | --- | --- |
| 0 | focused Prompt 14 selected gate | none | 43 selected generated contracts |

## Group Notes

- Construction: dynamic instantiation and constructor redeclaration smoke tests
  pass in the selected harness.
- Property read/write: focused public, typed, nullable, and clone-with property
  contracts pass. Dynamic property references and object-return property
  assignment remain broader corpus gaps.
- Method calls: private and protected in-class method calls pass in the selected
  harness.
- Visibility: focused private/protected visibility errors pass. Foreach over
  object properties remains a broader corpus visibility parity gap.
- Static access: focused static method/property contracts pass. Static property
  array initialization and static-property-as-instance-property cases remain
  broader corpus gaps.

## Recommendation

Prompt 14.2 class table and internal class lookup hygiene is implemented.
Class lookup names are normalized by trimming a leading namespace root slash and
ASCII-lowercasing the remaining name. PHP-visible display names preserve source
spelling without the leading root slash. IR and VM lookup use
`php_ir::module::normalize_class_name`; runtime-only class helpers use the same
rule through `php_runtime::normalize_class_name` without adding a runtime
dependency on IR.

Internal classes now use normalized lookup names while preserving display names
at object creation and PHP-visible APIs such as `get_class()`,
`get_debug_type()`, serialization, throwable messages, Reflection enum case
objects, `PhpToken`, Date/Time objects, and SPL helper objects. User classes,
IR-injected throwable/interface classes, runtime-created `stdClass`, `Closure`
metadata consumers, and standard-library helper objects all flow through the
same case-insensitive lookup rule.

Prompt 14.2 validation:

- `nix develop -c cargo test -p php_ir`: PASS
- `nix develop -c cargo test -p php_runtime object`: PASS
- `nix develop -c cargo test -p php_vm`: PASS
- `nix develop -c just phpt-dev-build`: PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 43 PASS, target 43 PASS.

Proceed to constructor/property/method behavior only after keeping these
lookup/display rules intact. Property references, complex assignment lowering,
foreach visibility parity, and static property cases remain explicit follow-up
blockers.

## Prompt 14.3 Basic Object Contracts

Prompt 14.3 adds generated PHPT contracts for:

- `constructor-property.phpt`
- `property-read-write.phpt`
- `method-call.phpt`
- `this-inside-method.phpt`

These contracts cover `new C`, `__construct`, public property read/write, public
instance method dispatch, basic method return values, and `$this` property state
inside methods. The VM unit suite already covers the same behavior in
`methods_execute_instance_calls_and_this_property`.

The focused selected module is green after using the generated Prompt 14
contract manifest as the module close gate. The broader php-src rows for
dynamic property references, complex assignment lowering, foreach visibility
parity, and static property edge cases remain corpus/backlog blockers.

Prompt 14.3 validation:

- `nix develop -c cargo test -p php_runtime object`: PASS
- `nix develop -c cargo test -p php_vm`: PASS
- `nix develop -c just phpt-dev-build`: PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_MANIFEST=tests/phpt/manifests/zend.objects-generated.jsonl nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 12 PASS, target 12 PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 43 PASS, target 43 PASS.

## Prompt 14.4 Visibility Error Routing

Prompt 14.4 adds generated PHPT contracts for:

- `private-property-external-error.phpt`
- `protected-method-external-error.phpt`
- `catch-error-visibility-error.phpt`

External private/protected property reads and method calls were already routed
through PHP `Error` objects. Private/protected property writes now use the same
runtime throwable routing path instead of returning the raw VM runtime error
directly, so `catch (Error)` can intercept write visibility failures too.

Remaining message wording gaps are limited to broader object gaps outside this
slice, especially foreach visibility output parity in
`tests/classes/visibility_005.phpt`.

Prompt 14.4 validation:

- `nix develop -c cargo test -p php_vm`: PASS
- `nix develop -c just phpt-dev-build`: PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_MANIFEST=tests/phpt/manifests/zend.objects-generated.jsonl nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 15 PASS, target 15 PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 43 PASS, target 43 PASS.

## Prompt 14.5 Static Access Contracts

Prompt 14.5 adds generated PHPT contracts for:

- `public-static-method.phpt`
- `static-property-read-write.phpt`
- `invalid-static-scope.phpt`

Public static methods, `self::`, `parent::`, and simple public static property
read/write already execute for the focused cases. Invalid `self::` static
method/property scope now routes through catchable PHP `Error` when it reaches
the VM, and the generated contract covers runtime-owned invalid static access:
undeclared static property read/write and non-static method called statically.

Late static binding remains limited to the existing focused metadata-backed
cases. The selected static property failures remain outside this slice because
they depend on broader property-reference and complex assignment lowering gaps.

Prompt 14.5 validation:

- `nix develop -c cargo test -p php_ir`: PASS
- `nix develop -c cargo test -p php_vm`: PASS
- `nix develop -c just phpt-dev-build`: PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_MANIFEST=tests/phpt/manifests/zend.objects-generated.jsonl nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 18 PASS, target 18 PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 43 PASS, target 43 PASS.

## Prompt 14.6 Typed Property Contracts

Prompt 14.6 adds generated PHPT contracts for:

- `typed-property-uninitialized.phpt`
- `nullable-property.phpt`
- `property-type-mismatch.phpt`

Object storage already distinguishes uninitialized typed properties from null.
Uninitialized instance/static typed property reads now route through catchable
PHP `Error`, and property type mismatches route through catchable PHP
`TypeError`. Nullable properties preserve null defaults and accept null writes
for the focused cases.

Prompt 14.6 validation:

- `nix develop -c cargo test -p php_runtime object`: PASS
- `nix develop -c cargo test -p php_vm`: PASS
- `nix develop -c just phpt-dev-build`: PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_MANIFEST=tests/phpt/manifests/zend.objects-generated.jsonl nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 21 PASS, target 21 PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 43 PASS, target 43 PASS.

## Prompt 14.7 Magic Method Contracts

Prompt 14.7 adds generated PHPT contracts for:

- `magic-get.phpt`
- `magic-set.phpt`
- `magic-isset.phpt`
- `magic-unset.phpt`
- `magic-call.phpt`
- `magic-call-static.phpt`
- `magic-invoke.phpt`
- `magic-to-string.phpt`

The focused magic method MVP supports missing-property `__get`, `__set`,
`__isset`, and `__unset`; missing instance and static method fallback through
`__call` and `__callStatic`; object invocation through `__invoke`; and
`__toString` in focused string concatenation/output contexts.

Recursive magic property and method dispatch is guarded by deterministic
runtime diagnostics:

- `E_PHP_VM_MAGIC_PROPERTY_RECURSION`
- `E_PHP_VM_MAGIC_METHOD_RECURSION`

Magic gaps remain outside this slice for serialization magic, full signature
validation parity, and wider edge cases such as reference-returning overloaded
properties.

Prompt 14.7 validation:

- `nix develop -c cargo test -p php_vm`: PASS
- `nix develop -c just phpt-dev-build`: PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_MANIFEST=tests/phpt/manifests/zend.objects-generated.jsonl nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 29 PASS, target 29 PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 43 PASS, target 43 PASS.

## Prompt 14.8 Clone and Clone-With Contracts

Prompt 14.8 adds generated PHPT contracts for:

- `clone-identity.phpt`
- `clone-independent-properties.phpt`
- `clone-magic-method.phpt`
- `clone-with-public-property.phpt`
- `clone-with-typed-property.phpt`
- `clone-with-type-mismatch.phpt`
- `clone-with-unsupported-private.phpt`

The focused clone MVP covers distinct clone identity, shallow object copies with
independent public property storage, and `__clone` dispatch. The focused
clone-with MVP covers public property replacement, typed public replacement
checks, and catchable `TypeError` for typed replacement mismatch.

Private, protected, readonly, and asymmetric setter replacement remain outside
the clone-with MVP. Unsupported replacement modifiers route through catchable
PHP `Error` with stable `E_PHP_VM_UNSUPPORTED_PROPERTY_MODIFIER` diagnostics
underneath.

Prompt 14.8 validation:

- `nix develop -c cargo test -p php_runtime object`: PASS
- `nix develop -c cargo test -p php_vm`: PASS
- `nix develop -c just phpt-dev-build`: PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_MANIFEST=tests/phpt/manifests/zend.objects-generated.jsonl nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 36 PASS, target 36 PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 43 PASS, target 43 PASS.

## Prompt 14.9 Trait and Enum Contracts

Prompt 14.9 adds generated PHPT contracts for:

- `trait-method.phpt`
- `trait-method-alias.phpt`
- `enum-unit-case.phpt`
- `enum-backed-case.phpt`
- `enum-cases.phpt`
- `enum-from-tryfrom.phpt`
- `enum-method.phpt`

The focused trait MVP covers trait method composition into a class and a simple
method alias. Wider trait semantics remain outside this slice: trait
properties, trait constants, nested trait uses, conflict resolution beyond the
focused alias path, and generator trait methods.

The focused enum MVP covers unit enum cases, backed enum cases and `value`,
`cases`, `from`, `tryFrom`, enum singleton identity through repeated case
access, and enum instance methods. Wider enum semantics remain outside this
slice for exhaustive `ValueError` parity, serialization/reflection completion,
interfaces beyond the current metadata surface, and edge-case diagnostics.

Prompt 14.9 validation:

- `nix develop -c cargo test -p php_runtime object`: PASS
- `nix develop -c cargo test -p php_vm`: PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_MANIFEST=tests/phpt/manifests/zend.objects-generated.jsonl nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 43 PASS, target 43 PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 43 PASS, target 43 PASS.

## Prompt 14.10 Closeout

Prompt 14.10 closes the object prompt sequence without starting Prompt 15 or
filesystem/stdlib work.

Closeout validation:

- `nix develop -c just verify-runtime`: PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.objects`: reference 43 PASS, target 43 PASS.
- `nix develop -c just phpt-full-fast`: FAIL. The full run completed 21,548
  PHPT tests with PASS 2,454, SKIP 9,071, XFAIL 4, FAIL 9,879, and BORK 140,
  then failed the no-regression comparison with 8,459 new or changed failure
  fingerprints. Artifacts are in
  `/private/tmp/phrust-phpt-work/full-runs/20260627T210436Z/`.

Object corpus tracking before and after Prompt 14 remains 178 PASS, 33 SKIP,
1,924 FAIL, and 0 BORK from 2,136 object/class corpus candidates. The selected
Prompt 14 contract manifest improved through the sequence and is now green at
reference 43 PASS and target 43 PASS.

The full-regression failure is not accepted as green and the full baseline was
not updated. The remaining corpus blockers are still dynamic property/static
property references, non-simple assignment lowering, and foreach visibility
parity over object properties.
