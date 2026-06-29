# closure.core

- Priority: 10
- Selected manifest: `tests/phpt/manifests/modules/closure.core.selected.jsonl`
- Focused selected counts: 33 PASS, 0 SKIP, 0 FAIL, 0 BORK from 33
  focused/generated fixtures
- Corpus triage counts: no standalone php-src corpus is assigned to this
  cross-module dashboard

## Scope

- `$GLOBALS`, dynamic globals, and top-level variable-table behavior
- array spread/unpack, key normalization, append tracking, and COW
- references, by-reference parameter sends, and foreach reference behavior
- include/require local scope and include path behavior
- function defaults, constant-expression defaults, variadics, and argument
  binding
- dynamic class, method, static method, and property dispatch
- selected late static binding in closures and static-member access
- selected fatal and warning output parity

## Non-Scope

- FPM, FastCGI, CGI, Apache module, phpdbg, or Zend extension ABI behavior
- complete SPL/extension iterator behavior
- broad standard-library completion
- full object magic method, serialization, reflection, and property-hook
  behavior

## Selected PHPT Fixture Groups

- `$GLOBALS` and dynamic globals:
  `dynamic-globals-alias.phpt`, `array_self_add_globals`, and dynamic variable
  smoke fixtures.
- Array spread/unpack and array/reference semantics:
  WordPress core-language spread/unpack fixtures plus the green
  `arrays.references` COW, key normalization, array element reference, and
  foreach fixtures.
- Include scope:
  `filesystem.streams` include return, include_once, include_path, and local
  scope fixtures.
- Functions and arguments:
  `zend.functions` defaults, constant-expression defaults, variadics,
  by-reference sends, dynamic first-class callables, and by-reference mismatch
  fixtures.
- Dynamic object/property and late static binding:
  WordPress dynamic class/method/static/property fixtures, object static
  member fixtures, and `closure.core/late-static-closure-binding.phpt`.
- Fatal and warning output:
  selected `diagnostics.output` fixtures for include warnings, undefined
  variables, array-to-string warnings, builtin arity/type errors, and invalid
  operands.

## Relevant Source Areas

- `crates/php_ir/`
- `crates/php_vm/`
- `crates/php_runtime/src/array.rs`
- `crates/php_runtime/src/reference.rs`
- `crates/php_runtime/src/globals.rs`
- `crates/php_runtime/src/context.rs`
- `crates/php_runtime/src/value.rs`
- `crates/php_runtime/src/object/`
- `crates/php_runtime/src/error_output.rs`

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=closure.core`
- `nix develop -c just verify-phpt`

## Known Gaps

- Full `$GLOBALS` and superglobal aliasing beyond the selected generated
  contracts remains owned by runtime global table and request-context work.
- Array/object/reference COW gaps involving property slots and static-property
  by-reference writes remain owned by `php_ir`, `php_runtime::reference`, and
  `php_runtime::object`.
- Advanced object behavior from the fresh `objects.classes` run remains open:
  magic methods, destructors/iterators, serialization, class constants,
  visibility stack formatting, and Reflection/autoload behavior.
- Fatal output parity still has broad path/stack formatting gaps outside the
  selected diagnostics contracts.

## Verification

Latest branch verification:

- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=closure.core`: PASS, reference 33 PASS and target 33 PASS.

## Next Step

Keep this selected dashboard green while implementing the closure core runtime
semantics prompts, then update
`docs/phpt/reports/closure-core-runtime-current.md` with each closeout run.
