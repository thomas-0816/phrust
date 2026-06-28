# spl

- Priority: 20
- Selected manifest: `tests/phpt/manifests/modules/spl.selected.jsonl`
- Prompt 20 generated counts: 8 PASS, 0 SKIP, 0 FAIL, 0 BORK from 8 selected fixtures
- Aggregate selected counts after adding prompt fixtures: 17 PASS, 2 SKIP, 189 FAIL, 0 BORK from 208 selected fixtures
- Full upstream corpus baseline: 39 PASS, 3 SKIP, 478 FAIL, 0 BORK from 520 corpus candidates

## Scope

- Prompt 20 generated SPL MVP submodule fixtures
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
- `tests/phpt/generated/spl.array-object/array-object-mvp.phpt`
- `tests/phpt/generated/spl.fixed-array/fixed-array-mvp.phpt`
- `tests/phpt/generated/spl.object-storage/object-storage-mvp.phpt`
- `tests/phpt/generated/spl.doubly-linked-list/linear-containers-mvp.phpt`
- `tests/phpt/generated/spl.file/file-classes-mvp.phpt`
- `tests/phpt/generated/spl.autoload/autoload-mvp.phpt`

## Relevant php-src Source Areas

- `ext/spl/php_spl.c`
- `ext/spl/spl_array.c`
- `ext/spl/spl_directory.c`
- `ext/spl/spl_dllist.c`
- `ext/spl/spl_fixedarray.c`
- `ext/spl/spl_observer.c`

## Target Gates

- Prompt submodules: `nix develop -c just phpt-dev-module MODULE=spl.<submodule>`
- Aggregate selected: `nix develop -c just phpt-dev-module MODULE=spl`
- `nix develop -c just diff-spl-reflection`
- `nix develop -c just verify-phpt`
- `nix develop -c just verify-stdlib`

The prompt submodule gates are green. The aggregate `spl` gate remains red
because the pre-existing upstream selected SPL batch still has 189 target
non-green outcomes; adding the prompt fixtures did not add BORKs or new prompt
fixture failures.

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
| `spl.autoload` | unknown from full corpus split | 0/0 | prepend/throw exactness and default `spl_autoload` namespace/path conventions |
| aggregate legacy selected batch | 196/0 | 189/0 | heaps, caching iterators, serialization, advanced autoload, catchable constructor `ValueError`s, FPM/daemon-style tests, full file APIs |

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

## Closed During Prompt 20

- `SplFileInfo::getExtension()` now covers leading-dot basenames such as `.test`.
- `json_encode(new SplFixedArray(...))` now emits array-shaped JSON instead of internal storage properties.
- Userland classes can implement internal SPL interfaces such as `Countable`,
  and `count($object)` dispatches to their `count()` method.
- `iterator_count()` and `iterator_to_array()` now cover array inputs and the
  existing Traversable/ArrayIterator MVP path.

## Next Step

Expand the selected manifests with upstream `ext/spl` tests one subarea at a time after each documented gap is closed.
