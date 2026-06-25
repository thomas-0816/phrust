# arrays.references

- Priority: 8
- Selected manifest: `tests/phpt/manifests/modules/arrays.references.selected.jsonl`
- Current counts: 26 PASS, 1 SKIP, 246 FAIL, 0 BORK from 273 corpus candidates

## Scope

- ordered arrays
- key conversion
- references
- copy-on-write
- foreach

## Non-Scope

- SPL collection classes

## Relevant PHPT Paths

- `tests/lang/returnByReference.009.phpt`
- `tests/lang/returnByReference.008.phpt`
- `tests/lang/returnByReference.007.phpt`
- `tests/lang/returnByReference.006.phpt`
- `tests/lang/returnByReference.005.phpt`
- `tests/lang/returnByReference.004.phpt`
- `tests/lang/returnByReference.003.phpt`
- `tests/lang/returnByReference.002.phpt`
- `tests/lang/passByReference_012.phpt`
- `tests/lang/passByReference_010.phpt`
- `tests/lang/passByReference_008.phpt`
- `tests/lang/passByReference_007.phpt`
- `tests/lang/passByReference_006.phpt`
- `tests/lang/passByReference_005.phpt`
- `tests/lang/passByReference_004.phpt`
- `tests/lang/passByReference_003.phpt`
- `tests/lang/passByReference_002.phpt`
- `tests/lang/passByReference_001.phpt`
- `tests/lang/foreach_with_object_001.phpt`
- `tests/lang/foreachLoopObjects.006.phpt`
- `tests/lang/foreachLoopObjects.005.phpt`
- `tests/lang/foreachLoopObjects.004.phpt`
- `tests/lang/foreachLoopObjects.003.phpt`
- `tests/lang/foreachLoopObjects.002.phpt`
- `tests/lang/foreachLoopObjects.001.phpt`
- `tests/lang/foreachLoopIteratorAggregate.004.phpt`
- `tests/lang/foreachLoopIteratorAggregate.003.phpt`
- `tests/lang/foreachLoopIteratorAggregate.002.phpt`
- `tests/lang/foreachLoopIteratorAggregate.001.phpt`
- `tests/lang/foreachLoopIterator.002.phpt`
- `tests/lang/foreachLoopIterator.001.phpt`
- `tests/lang/foreachLoop.017.phpt`
- `tests/lang/foreachLoop.016.phpt`
- `tests/lang/foreachLoop.015.phpt`
- `tests/lang/foreachLoop.014.phpt`
- `tests/lang/foreachLoop.013.phpt`
- `tests/lang/foreachLoop.012.phpt`
- `tests/lang/foreachLoop.011.phpt`
- `tests/lang/foreachLoop.009.phpt`
- `tests/lang/foreachLoop.006.phpt`

## Relevant php-src Source Areas

- `crates/php_runtime/`
- `crates/php_vm/`

## Target Gates

- `nix develop -c just phpt-module MODULE=standard.arrays`

## Known Gaps

- `runtime-unsupported-feature`: 145
- `runtime-error-or-diagnostic`: 80
- `runtime-output-mismatch`: 31
- `runtime-timeout`: 3
- `frontend-parse-or-compile`: 1

## Next Step

Close array data-model and reference/COW gaps before array builtins.
