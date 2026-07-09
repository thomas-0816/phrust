# spl

- Priority: 20
- Selected manifest: `tests/phpt/manifests/modules/spl.selected.jsonl`
- generated counts: 8 PASS, 0 SKIP, 0 FAIL, 0 BORK from 8 selected fixtures
- Aggregate selected counts after the latest selected-SPL parity pass: 208 PASS,
  1 SKIP, 0 FAIL, 0 BORK from 209 selected fixtures
- Full upstream corpus baseline: 39 PASS, 3 SKIP, 478 FAIL, 0 BORK from 520 corpus candidates

## Scope

- generated SPL MVP submodule fixtures
- Submodules:
  - `spl.interfaces`
  - `spl.array-iterator`
  - `spl.array-object`
  - `spl.fixed-array`
  - `spl.object-storage`
  - `spl.doubly-linked-list`
  - `spl.file`
  - `spl.autoload`

## Non-Scope

- full SPL API parity
- broad upstream `ext/spl` corpus parity
- heaps, priority queues, directory iterators, caching iterators, recursive iterator iterator, observer subject APIs, and serialization parity

## Selected PHPT Paths

- `tests/phpt/generated/spl.interfaces/interface-method-surface.phpt`
- `tests/phpt/generated/spl.array-iterator/iterator-mvps.phpt`
- `tests/phpt/generated/spl.array-iterator/iterator-helpers.phpt`
- `ext/spl/tests/iterator_to_array_array.phpt`
- `ext/spl/tests/iterator_count_array.phpt`
- `ext/spl/tests/spl_006.phpt`
- `ext/spl/tests/gh19577.phpt`
- `tests/phpt/generated/spl.array-object/array-object-mvp.phpt`
- `ext/spl/tests/spl_001.phpt`
- `tests/phpt/generated/spl.fixed-array/fixed-array-mvp.phpt`
- `ext/spl/tests/splfixedarray_json_encode.phpt`
- `tests/phpt/generated/spl.object-storage/object-storage-mvp.phpt`
- `ext/spl/tests/SplObjectStorage/SplObjectStorage_offsetGet.phpt`
- `tests/phpt/generated/spl.doubly-linked-list/linear-containers-mvp.phpt`
- `ext/spl/tests/SplDoublyLinkedList_current.phpt`
- `ext/spl/tests/SplDoublyLinkedList_key.phpt`
- `ext/spl/tests/SplDoublyLinkedList_isEmpty_empty.phpt`
- `ext/spl/tests/SplDoublyLinkedList_isEmpty_not-empty.phpt`
- `ext/spl/tests/SplDoublyLinkedList_offsetExists_success.phpt`
- `tests/phpt/generated/spl.file/file-classes-mvp.phpt`
- `ext/spl/tests/spl_fileinfo_getextension_leadingdot.phpt`
- `tests/phpt/generated/spl.autoload/autoload-mvp.phpt`
- `ext/spl/tests/spl_autoload_003.phpt`
- `ext/spl/tests/spl_autoload_010.phpt`
- `ext/spl/tests/spl_autoload_013.phpt`
- `ext/spl/tests/spl_autoload_bug48541.phpt`

## Relevant php-src Source Areas

- `ext/spl/php_spl.c`
- `ext/spl/spl_array.c`
- `ext/spl/spl_directory.c`
- `ext/spl/spl_dllist.c`
- `ext/spl/spl_fixedarray.c`
- `ext/spl/spl_observer.c`

## Target Gates

- SPL submodules: `nix develop -c just phpt-dev-module MODULE=spl.<submodule>`
- Aggregate selected: `nix develop -c just phpt-dev-module MODULE=spl`
- `nix develop -c just diff-spl-reflection`
- `nix develop -c just verify-phpt`
- `nix develop -c just verify-stdlib`

The SPL submodule gates are green. The aggregate `spl` selected upstream batch
is also green against the php-src oracle. The latest selected-SPL parity pass
fixed `iterator_to_array()` preserved-key diagnostics, iterator temporary
destructor/id reuse timing, CachingIterator string-cast fatal traces,
AppendIterator parent append rewinds, NoRewindIterator parent dispatch in dense
static calls, and SplFixedArray circular `debug_zval_dump()` output.

## Subarea Failure Snapshot

| Subarea | Before FAIL/BORK | Selected After FAIL/BORK | Remaining gaps |
| --- | ---: | ---: | --- |
| `spl.interfaces` | unknown from full corpus split | 0/0 | full interface inheritance and exact reflection metadata beyond selected methods |
| `spl.array-iterator` | unknown from full corpus split | 0/0 | flags, serialization, mutation by reference, deep recursive iterator APIs |
| `spl.array-object` | unknown from full corpus split | 0/0 | flags, object property mode, serialization, nested by-reference writes |
| `spl.fixed-array` | unknown from full corpus split | 0/0 | exact exception text and serialization |
| `spl.object-storage` | unknown from full corpus split | 0/0 | info edge cases, serialization, lvalue object-key bracket semantics |
| `spl.doubly-linked-list` | unknown from full corpus split | 0/0 | iterator mode matrix, serialization, exhaustive exception parity |
| `spl.file` | unknown from full corpus split | 0/0 | write-through file object semantics, locking, ownership/mode changes, CSV flag matrix, full seek modes |
| `spl.autoload` | unknown from full corpus split | 0/0 | throw exactness, destructor ordering, and default `spl_autoload` namespace/path conventions |
| aggregate legacy selected batch | 196/0 | 0/0 | selected upstream batch is green; remaining gaps are outside the selected corpus |

## Known Gaps

- `runtime-error-or-diagnostic`: 361 upstream SPL corpus candidates
- `runtime-unsupported-feature`: 71 upstream SPL corpus candidates
- `runtime-output-mismatch`: 60 upstream SPL corpus candidates
- `frontend-parse-or-compile`: 1 upstream SPL corpus candidate
- `STDLIB-GAP-SPL-INTERFACE-METHOD-SURFACES`
- `STDLIB-GAP-SPL-AUTOLOAD-ADVANCED`
- `STDLIB-GAP-SPL-OBJECT-HASH-PARITY`
- `STDLIB-GAP-SPL-ITERATOR-MUTATION-EDGES`
- `STDLIB-GAP-SPL-ITERATOR-FULL-API`
- `STDLIB-GAP-SPL-CONTAINER-FULL-API`
- `STDLIB-GAP-SPL-CONTAINER-NESTED-ARRAYACCESS`
- `STDLIB-GAP-SPL-FILE-FULL-API`
- `STDLIB-GAP-SPL-FILE-CSV-FLAGS`

## Implemented Surface

- `SplFileInfo::getExtension()` now covers leading-dot basenames such as `.test`.
- `json_encode(new SplFixedArray(...))` now emits array-shaped JSON instead of internal storage properties.
- Userland classes can implement internal SPL interfaces such as `Countable`,
  and `count($object)` dispatches to their `count()` method.
- `iterator_count()` and `iterator_to_array()` now cover array inputs and the
  existing Traversable/ArrayIterator MVP path.
- `SplObjectStorage` bracket assignment now accepts object keys for direct
  ArrayAccess attachment.
- `SplDoublyLinkedList` now covers selected upstream `isEmpty()`, empty
  `key()`, `current()`, and `offsetExists()` behavior.
- `eval()` now preserves concatenated code operands through HIR/IR lowering, so
  autoload callbacks can dynamically declare the requested class in
  `spl_autoload_bug48541.phpt`.
- Conditional class declarations inside function bodies are registered only
  when their declaration statement executes, and `spl_autoload_register()`
  honors the `prepend` flag used by `spl_autoload_010.phpt`.
- Closure debug metadata now includes parameter state, and
  `spl_autoload_functions()` exposes invokable object callbacks in the shape
  expected by `spl_autoload_013.phpt`.
- Dense method dispatch now routes SPL runtime classes and SPL runtime
  subclasses through the engine-owned SPL handlers, including heap methods.
- Dense static dispatch now handles SPL subclass `parent::__construct()` and
  `parent::method()` calls by initializing or invoking the parent SPL runtime
  storage on the current `$this`.
- `iterator_to_array()` now emits PHP-compatible deprecations for preserved
  float and null keys and throws the PHP array-offset error for array keys.
- Iterator builtins now release temporary iterator arguments with PHP-compatible
  destructor timing for direct and dynamic calls.
- CachingIterator string casts now report uncaught `BadMethodCallException`
  traces with the synthetic `CachingIterator->__toString()` frame.
- AppendIterator parent `append()` dispatch rewinds newly attached iterators at
  append time, and dense static parent dispatch uses the VM-aware SPL handlers.
- `debug_zval_dump()` now renders SplFixedArray logical slots rather than
  backing storage properties, including circular slot recursion.

## Next Step

Promote the next focused upstream SPL subarea from the full corpus into the
selected manifest and close any newly exposed behavior families against the
php-src oracle.
