# SPL Current PHPT Report

Generated: 2026-06-28T06:18:00Z

## Scope

This report covers the generated SPL selected module and its prompt-20
submodules:

- `spl.interfaces`
- `spl.array-iterator`
- `spl.array-object`
- `spl.fixed-array`
- `spl.object-storage`
- `spl.doubly-linked-list`
- `spl.file`
- `spl.autoload`

## Selected PHPT Results

| Module | Before selected FAIL/BORK | After selected PASS | After selected FAIL | After selected BORK | Remaining gaps |
| --- | ---: | ---: | ---: | ---: | --- |
| `spl.interfaces` | not tracked | 1 | 0 | 0 | full interface contracts, inherited method metadata details |
| `spl.array-iterator` | not tracked | 1 | 0 | 0 | sorting flags, serialization, recursive edge cases |
| `spl.array-object` | not tracked | 1 | 0 | 0 | property-backed storage, flags, serialization |
| `spl.fixed-array` | not tracked | 1 | 0 | 0 | resizing edge cases, conversion parity, serialization |
| `spl.object-storage` | not tracked | 1 | 0 | 0 | info edge cases, serialization, object-key bracket syntax |
| `spl.doubly-linked-list` | not tracked | 1 | 0 | 0 | iterator mode flags, heap/priority queue variants |
| `spl.file` | not tracked | 1 | 0 | 0 | CSV flags, file locking, full stream-wrapper behavior |
| `spl.autoload` | not tracked | 1 | 0 | 0 | include-path scanning, extension lists, error ordering |
| `spl` aggregate selected | 196/0 | 17 | 189 | 0 | legacy upstream selected SPL failures remain; prompt fixtures add no failures |

The pre-split full upstream SPL baseline remains documented in
`docs/phpt/modules/spl.md`: 39 PASS, 3 SKIP, 478 FAIL, 0 BORK across 520
candidate tests. The selected submodule fixtures are new prompt-20 contracts, so
there is no prior selected FAIL/BORK baseline for the submodule rows. The
aggregate selected manifest now contains the existing 200 upstream selected
tests plus 8 prompt fixtures; the target result is 17 PASS, 2 SKIP, 189 FAIL,
0 BORK.

## Verification

Passed:

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=spl.interfaces`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=spl.array-iterator`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=spl.array-object`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=spl.fixed-array`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=spl.object-storage`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=spl.doubly-linked-list`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=spl.file`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=spl.autoload`
- `nix develop -c just diff-spl-reflection`
- `nix develop -c just verify-phpt`
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php nix develop -c just verify-stdlib`
- `nix develop -c cargo test -p php_std`
- `nix develop -c cargo test -p php_vm`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-fast MODULE=spl FILE=ext/spl/tests/spl_fileinfo_getextension_leadingdot.phpt`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-fast MODULE=spl FILE=ext/spl/tests/splfixedarray_json_encode.phpt`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-fast MODULE=spl FILE=ext/spl/tests/spl_002.phpt`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-fast MODULE=spl FILE=ext/spl/tests/iterator_count_array.phpt`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-fast MODULE=spl FILE=ext/spl/tests/iterator_to_array_array.phpt`
- `nix develop -c cargo test -p php_runtime json`
- `nix develop -c cargo test -p php_vm spl_file_info_and_file_object_use_allowed_local_files`
- `nix develop -c cargo test -p php_vm spl_fixed_array_supports_bounds_checked_array_access`
- `nix develop -c cargo test -p php_vm spl_userland_countable_uses_internal_interface_metadata`
- `nix develop -c cargo test -p php_vm spl_iterator_functions_cover_arrays_and_array_iterator_mvp`

Completed but not green:

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=spl`

The aggregate SPL selected run completed reference-side with 206 PASS, 2 SKIP
and target-side with 17 PASS, 2 SKIP, 189 FAIL, 0 BORK. The failures are the
pre-existing upstream selected SPL gaps; the eight prompt fixtures are green in
their submodule gates and were reused as PASS in the aggregate run. Nine
additional upstream selected tests also passed during prompt 20:
`splfixedarray_json_encode.phpt`,
`spl_fileinfo_getextension_leadingdot.phpt`, `spl_autoload_003.phpt`,
`spl_006.phpt`, `spl_002.phpt`, `spl_001.phpt`,
`iterator_to_array_array.phpt`, `iterator_count_array.phpt`, and
`gh19577.phpt`.

Not completed:

- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php nix develop -c just phpt-full-fast`

The corrected-reference full fast regression was not rerun. An earlier
full-fast attempt was stopped after roughly 11 minutes because the remaining
non-reused target workers were advancing slowly through unsupported
FPM/daemon-style PHPTs, moving only from about case `20490` to `20548`. No
summary was produced, and no full-regression pass is claimed here.

## Source Integrity

`verify-phpt` and each `phpt-dev-module` run verified 24,469 php-src manifest
entries. Seven platform-generated php-src artifacts were explicitly skipped by
name because a clean local PHP 8.5.7 build regenerates them on this host:

- `Zend/zend_ini_parser.c`
- `Zend/zend_ini_parser.h`
- `Zend/zend_language_parser.c`
- `Zend/zend_language_parser.h`
- `ext/json/json_parser.tab.h`
- `ext/opcache/jit/ir/ir_emit_aarch64.h`
- `main/build-defs.h`
