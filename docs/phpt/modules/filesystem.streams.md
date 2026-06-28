# filesystem.streams

- Priority: 11
- Selected manifest: `tests/phpt/manifests/modules/filesystem.streams.selected.jsonl`
- Current counts: 11 PASS, 0 SKIP, 0 FAIL, 0 BORK from 11 selected module fixtures

## Scope

- local filesystem
- php://memory streams
- resources
- include_path
- include/require

## Non-Scope

- network streams
- PHAR streams
- extension-backed wrappers
- user stream wrappers

## Selected PHPT Fixtures

- `tests/phpt/generated/filesystem.streams/local-file-roundtrip.phpt`
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
- Additional local file, directory, cwd, and warning/error PHPTs should be
  added as deterministic selected fixtures before expanding this module count.

## Next Step

Prompt 15 is closed at 11 selected fixtures passing on reference and target.
Continue with Prompt 16.1 standard-core dashboard.

## Prompt 15.2 Harness Report

The focused `filesystem.streams` PHPT harness covers the Prompt 15.2 seed
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
  Prompt 15.6

Prompt 15.2 validation:

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=filesystem.streams`: reference 7 PASS, target 7 PASS
- `nix develop -c just verify-phpt`: PASS

Blocker report: no blockers in the focused generated harness. The next
expansion points are the Prompt 15.3 request-local state checks and later
require/include_once/include_path edge cases.

## Prompt 15.3 Request-Local State Report

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

Prompt 15.3 validation:

- `nix develop -c cargo test -p php_runtime`: PASS, 178 tests
- `nix develop -c cargo test -p php_vm`: PASS, 339 tests
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=filesystem.streams`: reference 7 PASS, target 7 PASS

Blocker report: no Prompt 15.3 blockers found. JSON/PCRE last-error request
state was not changed for this prompt; JSON state is owned by Prompt 17.2.

## Prompt 15.4 Local Filesystem Report

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

Prompt 15.4 validation:

- `nix develop -c just diff-streams`: PASS, total=2 pass=2 fail=0 skip=0 known_gap=0
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=filesystem.streams`: reference 8 PASS, target 8 PASS

Blocker report: no blockers in the focused generated local filesystem harness.
Network URLs remain outside scope. Broader byte-for-byte warning parity for
additional filesystem failure modes should be added as selected fixtures before
expanding beyond deterministic local paths.

## Prompt 15.5 Streams and Resources Report

The focused stream/resource surface is covered by deterministic generated
fixtures:

- `fopen`, `fclose`, `fread`, `fwrite`, local file handles, and persisted file
  contents: `local-file-resource.phpt`
- `php://memory`: `php-memory-stream.phpt` and `stream-seek-contents.phpt`
- `php://temp`: `php-temp-stream.phpt`
- `feof`, `ftell`, `fseek`, `rewind`, and `stream_get_contents`:
  `stream-seek-contents.phpt`
- `stream_get_meta_data`: `local-file-resource.phpt`,
  `php-memory-stream.phpt`, and `php-temp-stream.phpt`

Prompt 15.5 validation:

- `nix develop -c cargo test -p php_runtime resource`: PASS, 6 tests
- `nix develop -c cargo test -p php_vm`: PASS, 339 tests
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=filesystem.streams`: reference 9 PASS, target 9 PASS

Blocker report: no blockers in the focused stream/resource harness. Network
streams and stream filters remain outside scope.

## Prompt 15.6 Include/Require Local Semantics Report

The focused include/require surface is covered by deterministic generated
fixtures:

- include return values, shared top-level local scope, include_once execution,
  require_once execution, and include_path lookup:
  `include-local-semantics.phpt`
- missing local require warning/fatal stdout shape:
  `require-missing-fatal.phpt`
- existing include_path include return baseline: `include-path-scope.phpt`

Prompt 15.6 VM changes route include failure diagnostics through the existing
single frontend/IR/VM pipeline, attach the include instruction span to the
structured diagnostic, and render PHP-style missing-file warning/fatal output
when `display_errors` and `error_reporting` allow it. The VM still keeps its
structured runtime diagnostics on stderr for PHPT target debugging; the fatal
require PHPT captures stdout only to compare the PHP-visible output stream.

Prompt 15.6 validation:

- `nix develop -c cargo test -p php_vm include -- --nocapture`: PASS, 15 tests
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=filesystem.streams`: reference 11 PASS, target 11 PASS

Blocker report: no blockers in the focused local include/require harness.
Remote wrappers, PHAR, and user stream wrappers remain outside scope.

## Prompt 15.7 Closeout Report

Prompt 15 closed with the focused `filesystem.streams` harness at 11 selected
fixtures, all passing on both the reference PHP binary and the target VM. The
module covers deterministic local file operations, cwd/include_path state,
local/php stream resources, include return/scope/once behavior, include_path
lookup, and missing require warning/fatal output.

Prompt 15.7 validation:

- `nix develop -c just verify-runtime`: PASS
- `nix develop -c just verify-stdlib`: PASS
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=filesystem.streams`: reference 11 PASS, target 11 PASS

No new known-gap IDs were added for Prompt 15. Network streams, PHAR,
extension-backed wrappers, user stream wrappers, and stream filters remain
outside this module scope.
