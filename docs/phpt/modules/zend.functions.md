# zend.functions

- Priority: 9
- Selected manifest: `tests/phpt/manifests/modules/zend.functions.selected.jsonl`
- Focused selected counts: 29 PASS, 0 SKIP, 0 FAIL, 0 BORK from 29 generated contract fixtures
- Corpus triage counts: 85 PASS, 53 SKIP, 727 FAIL, 0 BORK from 887 corpus
  candidates

## Scope

- user functions
- closures
- callables
- arity
- type coercion

## Non-Scope

- Reflection API surface

## Selected PHPT Fixture Groups

- builtin arginfo arity and scalar coercion
- user-function defaults, surplus arguments, variadics, and missing required
  arguments
- by-reference local and array-element sends
- Closure runtime class and `Closure::fromCallable` basics
- first-class callables, callable arrays, `call_user_func`, `is_callable`, and
  callable type checks
- pipe RHS callable dispatch

## Relevant php-src Source Areas

- `crates/php_semantics/`
- `crates/php_runtime/`
- `crates/php_vm/`

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.functions`

## Known Gaps

- The selected contract gate is green at 29 PASS for both reference
  and target.
- The broader 200-row php-src blocker slice remains documented in
  `docs/phpt/reports/zend.functions-current.md` and is kept as backlog
  analysis, not the module close gate.
- Remaining corpus gaps include constant-expression closure/property
  initializers, by-reference returns, relative type contexts, direct invalid
  callable-array parity, and wider Closure metadata/output parity.

## Wave 4 Closeout

The core language/object promotion branch did not need new `zend.functions`
manifest rows. The closeout gate remains green:

- Gate:
  `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.functions`
- Reference: 29 PASS, 0 non-green.
- Target: 29 PASS, 0 non-green.
- Source integrity: 24475 php-src manifest entries checked, 0 skipped.

## Next Step

The selected gate is closed for the selected generated function/callable contracts.
Keep those contracts green while reducing the broader php-src blocker slice.
