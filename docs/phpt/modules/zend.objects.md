# zend.objects

- Priority: 10
- Selected manifest: `tests/phpt/manifests/modules/zend.objects.selected.jsonl`
- Focused selected counts: 43 PASS, 0 SKIP, 0 FAIL, 0 BORK from 43 generated contract fixtures
- Corpus triage counts: 178 PASS, 33 SKIP, 1924 FAIL, 0 BORK from 2136
  object/class corpus candidates

## Scope

- construction
- property read/write
- method calls
- visibility
- static access
- magic method MVP
- clone/clone-with MVP
- trait method MVP
- enum case/static method MVP

## Non-Scope

- complete trait semantics
- complete enum semantics
- Reflection API completion

## Selected PHPT Fixture Groups

- upstream-derived object smoke fixtures
- constructor, public property, public method, and `$this` basics
- visibility error routing
- static method/property basics and invalid static access
- typed and nullable property basics
- focused magic methods
- clone and clone-with MVP
- focused trait method and enum contracts

## Relevant Source Areas

- `crates/php_semantics/`
- `crates/php_runtime/`
- `crates/php_vm/`

## Target Gates

- `nix develop -c just phpt-generate-module MODULE=zend.objects`
- `nix develop -c just phpt-dev-module MODULE=zend.objects`
- `nix develop -c just verify-phpt`
- `nix develop -c just verify-runtime`
- `nix develop -c cargo test -p php_ir`
- `nix develop -c cargo test -p php_runtime object`
- `nix develop -c cargo test -p php_vm`

## Known Gaps

- focused selected manifest includes generated object contracts and is green at
  43 PASS and 0 non-green outcomes for both reference and target
- the broader php-src seed rows that previously exposed dynamic property
  references, object-return assignment lowering, foreach visibility parity, and
  static-as-instance edge cases are retained as documented corpus/backlog gaps
  instead of the selected close-gate fixtures
- `nix develop -c just verify-phpt` passes
- class lookup hygiene passes `php_ir`, `php_runtime object`, and
  `php_vm` cargo tests
- basic object contracts pass for constructor property
  initialization, public property read/write, public method calls, and `$this`
  state inside methods
- validation passes `php_runtime object`, `php_vm`, and the
  generated `zend.objects` PHPT manifest
- visibility errors route private/protected property reads/writes
  and private/protected method calls through catchable PHP `Error`
- static contracts pass public static methods, simple static
  property read/write, and catchable invalid static access
- typed property contracts pass uninitialized property `Error`,
  nullable property reads/writes, and property type mismatch `TypeError`
- magic method contracts pass focused `__get`, `__set`,
  `__isset`, `__unset`, `__call`, `__callStatic`, `__invoke`, and
  `__toString` behavior
- recursion guards emit deterministic
  `E_PHP_VM_MAGIC_PROPERTY_RECURSION` and
  `E_PHP_VM_MAGIC_METHOD_RECURSION` diagnostics
- clone contracts pass distinct identity, independent public
  properties, and focused `__clone` dispatch
- clone-with contracts pass public property replacement, typed
  public replacement checks, catchable typed mismatch `TypeError`, and
  catchable unsupported private replacement `Error`
- private/protected/readonly/asymmetric setter clone-with replacement remains
  outside the MVP
- trait contracts pass focused trait method composition and simple
  method aliasing
- enum contracts pass unit cases, backed cases, `cases`, `from`,
  `tryFrom`, and enum instance methods
- validation passes `nix develop -c just verify-runtime`
- focused module gate passes
  `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.objects`
- full regression was run with `nix develop -c just
  phpt-full-fast`; it completed 21,548 PHPT tests but failed the no-regression
  comparison with 8,459 new or changed failure fingerprints, so the full
  baseline was not updated
- trait properties, trait constants, nested trait uses, full conflict
  resolution, and generator trait methods remain outside the trait MVP
- exhaustive enum diagnostic parity, serialization/reflection completion, and
  broader enum interface behavior remain outside the enum MVP
- serialization magic, full magic method signature parity, and
  reference-returning overloaded properties remain outside this scope
- dynamic property references and object-return property assignment
- foreach visibility over object properties
- static property initialization and static-property-as-instance-property access

## Focused Blockers

No blockers remain in the selected manifest.

Corpus/backlog blockers outside the selected gate:

- `E_PHP_IR_UNSUPPORTED_PROPERTY_REFERENCE`: dynamic property references and
  static-property-as-instance-property cases.
- `E_PHP_IR_UNSUPPORTED_HIR_STATEMENT`: object-return property assignment and
  static property array initialization.
- output mismatch without stable `E_PHP_*` ID: foreach visibility filtering over
  object properties.

## Next Step

The selected gate is closed for the focused generated object contracts. Keep
the class lookup rules intact and selected object contracts green while closing
the remaining corpus property/reference, assignment-lowering, and foreach
visibility blockers. Normalized class lookup names are case-insensitive and
root-slash-free, and display names preserve PHP-visible source spelling.
