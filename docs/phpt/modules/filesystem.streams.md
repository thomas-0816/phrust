# filesystem.streams

- Priority: 11
- Selected manifest: `tests/phpt/manifests/modules/filesystem.streams.selected.jsonl`
- Current target counts: 35 PASS, 10 SKIP, 0 FAIL, 0 BORK from 45 selected
  module fixtures

## Scope

- local filesystem
- php://memory streams
- resources
- include_path
- include/require
- selected upstream local file, stat, temp, glob, readfile, and
  `stream_get_contents` behavior
- selected upstream `basename`, `dirname`, and `pathinfo` behavior

## Non-Scope

- network streams
- PHAR streams
- extension-backed wrappers
- user stream wrappers

## Selected PHPT Fixtures

- `tests/phpt/generated/filesystem.streams/local-file-roundtrip.phpt`
- `ext/standard/tests/file/file_get_contents_basic.phpt`
- `ext/standard/tests/file/file_get_contents_basic001.phpt`
- `ext/standard/tests/file/file_get_contents_file_put_contents_basic.phpt`
- `ext/standard/tests/file/file_get_contents_variation7.phpt`
- `ext/standard/tests/file/file_put_contents_variation1.phpt`
- `ext/standard/tests/file/readfile_basic.phpt`
- `ext/standard/tests/file/readfile_variation9.phpt`
- `ext/standard/tests/file/chmod_variation1.phpt`
- `ext/standard/tests/file/glob_basic.phpt`
- 20 target-green upstream `basename`, `dirname`, and `pathinfo` fixtures from
  `ext/standard/tests/file`
- `ext/standard/tests/file/lstat_stat_variation1.phpt`
- `ext/standard/tests/file/lstat_stat_variation2.phpt`
- `ext/standard/tests/file/tempnam_variation5.phpt`
- `ext/standard/tests/file/touch_variation2.phpt`
- `ext/standard/tests/streams/stream_get_contents_001.phpt`
- `tests/phpt/generated/filesystem.streams/php-memory-stream.phpt`
- `tests/phpt/generated/filesystem.streams/include-path-scope.phpt`
- `tests/phpt/generated/filesystem.streams/directory-cwd-roundtrip.phpt`
- `tests/phpt/generated/filesystem.streams/local-file-resource.phpt`
- `tests/phpt/generated/filesystem.streams/missing-file-warnings.phpt`
- `tests/phpt/generated/filesystem.streams/php-temp-stream.phpt`
- `tests/phpt/generated/filesystem.streams/local-filesystem-mutations.phpt`
- `tests/phpt/generated/filesystem.streams/stream-seek-contents.phpt`
- `tests/phpt/generated/filesystem.streams/include-local-semantics.phpt`
- `tests/phpt/generated/filesystem.streams/require-missing-fatal.phpt`

## Relevant php-src Source Areas

- `ext/standard/tests/file/`
- `ext/standard/tests/streams/`
- `crates/php_runtime/`

## Target Gates

- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=filesystem.streams`
- `nix develop -c just verify-phpt`

## Known Gaps

- Network streams, PHAR streams, extension-backed wrappers, and user stream
  wrappers are outside this module contract.
- Broader `file_get_contents`/`readfile` argument diagnostics, stat metadata
  helpers, stream metadata shape, symlink/link helpers, and warning text parity
  remain outside this selected gate.

## Next Step

The selected gate is closed at 25 selected fixtures passing on reference and target.
Continue with standard-core dashboard.

## Harness Report

The focused `filesystem.streams` PHPT harness covers the selected seed
areas with generated deterministic fixtures:

- cwd: `directory-cwd-roundtrip.phpt`
- `file_exists`: `local-file-roundtrip.phpt`
- `file_get_contents`: `local-file-roundtrip.phpt`,
  `local-file-resource.phpt`, `missing-file-warnings.phpt`
- `file_put_contents`: `local-file-roundtrip.phpt`,
  `include-path-scope.phpt`
- `fopen`/`fread`/`fwrite`/`fclose`: `local-file-resource.phpt`,
  `php-memory-stream.phpt`, `php-temp-stream.phpt`
- include/require local files: `include-path-scope.phpt` covers include path
  lookup and include return values; require-specific behavior remains for
  the selected gate

Validation:

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=filesystem.streams`: reference 7 PASS, target 7 PASS
- `nix develop -c just verify-phpt`: PASS

Blocker report: no blockers in the focused generated harness. The next
expansion points are the selected request-local state checks and later
require/include_once/include_path edge cases.

## Request-Local State Report

The VM owns request-local builtin state on `ExecutionState`: cwd, INI-backed
include_path, and the resource table. Builtin dispatch creates a temporary
`BuiltinContext` for each call, passes the request resource table by mutable
reference, seeds include_path from request INI, and writes the mutated cwd back
after the builtin returns.

Focused VM coverage now asserts:

- `chdir` then `getcwd` persists cwd across later builtin calls.
- `fopen` then `fwrite`/`rewind`/`fread`/`fclose` preserves stream resource
  identity across calls.
- `ini_set('include_path', ...)` feeds later `stream_resolve_include_path` and
  remains visible through `ini_get`.

Validation:

- `nix develop -c cargo test -p php_runtime`: PASS, 178 tests
- `nix develop -c cargo test -p php_vm`: PASS, 339 tests
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=filesystem.streams`: reference 7 PASS, target 7 PASS

Blocker report: no selected blockers found. JSON/PCRE last-error request
state was not changed for this scope; JSON state is tracked in the JSON module.

## Local Filesystem Report

The focused local filesystem surface is covered by deterministic generated
fixtures:

- `file_exists`, `file_get_contents`, `file_put_contents`, `is_file`,
  `filesize`, and `unlink`: `local-file-roundtrip.phpt`
- `is_dir`, `mkdir`, `rmdir`, `getcwd`, and `chdir`:
  `directory-cwd-roundtrip.phpt`
- `filemtime`, `readfile`, and `rename`:
  `local-filesystem-mutations.phpt`
- Missing local-file warnings: `missing-file-warnings.phpt` covers
  `file_get_contents` and `fopen` warning output

Validation:

- `nix develop -c just diff-streams`: PASS, total=2 pass=2 fail=0 skip=0 known_gap=0
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=filesystem.streams`: reference 8 PASS, target 8 PASS

Blocker report: no blockers in the focused generated local filesystem harness.
Network URLs remain outside scope. Broader byte-for-byte warning parity for
additional filesystem failure modes should be added as selected fixtures before
expanding beyond deterministic local paths.

## Streams and Resources Report

The focused stream/resource surface is covered by deterministic generated
fixtures:

- `fopen`, `fclose`, `fread`, `fwrite`, local file handles, and persisted file
  contents: `local-file-resource.phpt`
- `php://memory`: `php-memory-stream.phpt` and `stream-seek-contents.phpt`
- `php://temp`: `php-temp-stream.phpt`
- `feof`, `ftell`, `fseek`, `rewind`, and `stream_get_contents`:
  `stream-seek-contents.phpt`; the selected fixture covers `SEEK_SET`,
  `SEEK_CUR`, `SEEK_END`, invalid negative targets, and invalid `whence`
  values
- `stream_get_meta_data`: `local-file-resource.phpt`,
  `php-memory-stream.phpt`, and `php-temp-stream.phpt`

Validation:

- `nix develop -c cargo test -p php_runtime resource`: PASS, 6 tests
- `nix develop -c cargo test -p php_vm`: PASS, 339 tests
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=filesystem.streams`: reference 9 PASS, target 9 PASS

Blocker report: no blockers in the focused stream/resource harness. Network
streams and stream filters remain outside scope.

## Include/Require Local Semantics Report

The focused include/require surface is covered by deterministic generated
fixtures:

- include return values, shared top-level local scope, include_once execution,
  require_once execution, and include_path lookup:
  `include-local-semantics.phpt`
- missing local require warning/fatal stdout shape:
  `require-missing-fatal.phpt`
- existing include_path include return baseline: `include-path-scope.phpt`

VM changes route include failure diagnostics through the existing
single frontend/IR/VM pipeline, attach the include instruction span to the
structured diagnostic, and render PHP-style missing-file warning/fatal output
when `display_errors` and `error_reporting` allow it. The VM still keeps its
structured runtime diagnostics on stderr for PHPT target debugging; the fatal
require PHPT captures stdout only to compare the PHP-visible output stream.

Validation:

- `nix develop -c cargo test -p php_vm include -- --nocapture`: PASS, 15 tests
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=filesystem.streams`: reference 11 PASS, target 11 PASS

Blocker report: no blockers in the focused local include/require harness.
Remote wrappers, PHAR, and user stream wrappers remain outside scope.

## Validation Report

The selected gate closed with the focused `filesystem.streams` harness at 11 selected
fixtures, all passing on both the reference PHP binary and the target VM. The
module covers deterministic local file operations, cwd/include_path state,
local/php stream resources, include return/scope/once behavior, include_path
lookup, and missing require warning/fatal output.

Validation:

- `nix develop -c just verify-runtime`: PASS
- `nix develop -c just verify-stdlib`: PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=filesystem.streams`: reference 25 PASS, target 25 PASS

No new known-gap IDs were added for this selected gate. Network streams, PHAR,
extension-backed wrappers, user stream wrappers, and stream filters remain
outside this module scope.

## Request Filesystem Overlay

The `wp.request-filesystem` overlay adds selected coverage for permission/stat
helpers, `sys_get_temp_dir`, `tempnam`, `tmpfile`, request-local `umask`,
directory iteration, stream context defaults/options, and local
`stream_set_timeout` behavior without adding overlay-specific rows to
`filesystem.streams`.

## Path Helper Report

The selected upstream path-helper slice now covers PHP-compatible
`basename`, `dirname`, and `pathinfo` behavior, including suffix stripping,
empty-path dirname handling, dotfile extension splitting, and combined
`pathinfo` option priority.

Validation:

- `nix develop -c cargo test -p php_runtime path_helpers --no-fail-fast`: PASS, 1 test
- `nix develop -c cargo test -p php_runtime filesystem --no-fail-fast`: PASS, 6 tests
- `PHP_SRC_DIR=$PHP_SRC_DIR PHPT_MANIFEST=/tmp/phrust-filesystem-path-helpers.jsonl PHPT_REUSE_LAST=0 PHPT_TIMEOUT_SECONDS=10 nix develop -c just phpt-module-target MODULE=filesystem.streams`: PASS, 10 PASS, 10 SKIP

Blocker report: no non-green outcomes remain in the focused
`basename`/`dirname`/`pathinfo` upstream subset. The 10 SKIP rows are
Windows-only upstream PHPTs.
