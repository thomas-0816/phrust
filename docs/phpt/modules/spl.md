# spl

- Priority: 20
- Selected manifest: `tests/phpt/manifests/modules/spl.selected.jsonl`
- Current counts: 39 PASS, 3 SKIP, 478 FAIL, 0 BORK from 520 corpus candidates

## Scope

- core SPL interfaces and common collections

## Non-Scope

- full SPL API parity

## Relevant PHPT Paths

- `ext/spl/tests/unserialize_errors.phpt`
- `ext/spl/tests/unserialize.phpt`
- `ext/spl/tests/splfixedarray_json_encode.phpt`
- `ext/spl/tests/spl_recursive_iterator_iterator_key_case.phpt`
- `ext/spl/tests/spl_pqueue_gc.phpt`
- `ext/spl/tests/spl_pq_top_error_empty.phpt`
- `ext/spl/tests/spl_pq_top_error_corrupt.phpt`
- `ext/spl/tests/spl_pq_top_basic.phpt`
- `ext/spl/tests/spl_limit_iterator_check_limits.phpt`
- `ext/spl/tests/spl_iterator_to_array_error.phpt`
- `ext/spl/tests/spl_iterator_recursive_getiterator_error.phpt`
- `ext/spl/tests/spl_iterator_iterator_constructor.phpt`
- `ext/spl/tests/spl_iterator_getcallchildren.phpt`
- `ext/spl/tests/spl_iterator_caching_getcache_error.phpt`
- `ext/spl/tests/spl_iterator_caching_count_error.phpt`
- `ext/spl/tests/spl_iterator_caching_count_basic.phpt`
- `ext/spl/tests/spl_iterator_apply_with_trampoline.phpt`
- `ext/spl/tests/spl_iterator_apply_error_001.phpt`
- `ext/spl/tests/spl_iterator_apply_error.phpt`
- `ext/spl/tests/spl_heap_iteration_error.phpt`
- `ext/spl/tests/spl_heap_isempty.phpt`
- `ext/spl/tests/spl_heap_is_empty_basic.phpt`
- `ext/spl/tests/spl_heap_count_basic.phpt`
- `ext/spl/tests/spl_fileinfo_getlinktarget_basic.phpt`
- `ext/spl/tests/spl_fileinfo_getextension_leadingdot.phpt`
- `ext/spl/tests/spl_cachingiterator___toString_basic.phpt`
- `ext/spl/tests/spl_caching_iterator_constructor_flags.phpt`
- `ext/spl/tests/spl_autoload_warn_on_false_do_throw.phpt`
- `ext/spl/tests/spl_autoload_unregister_without_registrations.phpt`
- `ext/spl/tests/spl_autoload_throw_with_spl_autoloader_call_as_autoloader.phpt`
- `ext/spl/tests/spl_autoload_called_scope.phpt`
- `ext/spl/tests/spl_autoload_call_basic.phpt`
- `ext/spl/tests/spl_autoload_bug48541.phpt`
- `ext/spl/tests/spl_autoload_014.phpt`
- `ext/spl/tests/spl_autoload_013.phpt`
- `ext/spl/tests/spl_autoload_012.phpt`
- `ext/spl/tests/spl_autoload_011.phpt`
- `ext/spl/tests/spl_autoload_010.phpt`
- `ext/spl/tests/spl_autoload_009.phpt`
- `ext/spl/tests/spl_autoload_008.phpt`

## Relevant php-src Source Areas

- `ext/spl/tests/`

## Target Gates

- `nix develop -c just phpt-module MODULE=spl`

## Known Gaps

- `runtime-error-or-diagnostic`: 361
- `runtime-unsupported-feature`: 71
- `runtime-output-mismatch`: 60
- `frontend-parse-or-compile`: 1

## Next Step

Build on stable object, array, iterator, and filesystem layers.
