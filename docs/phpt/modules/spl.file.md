# spl.file

- Priority: 20
- Selected manifest: `tests/phpt/manifests/modules/spl.file.selected.jsonl`
- Current selected counts: 1 PASS, 0 SKIP, 0 FAIL, 0 BORK

## Scope

- `SplFileInfo`
- `SplFileObject`
- `SplTempFileObject`
- `getPathname`
- `getFilename`
- `isFile` and `isDir`
- `openFile` where covered by the runtime constructor path
- `fgets`
- foreach lines MVP
- root-constrained local files

## Non-Scope

- locking
- chmod/chown
- CSV full flag matrix
- full seek modes
- write-through `SplFileObject` semantics

## Selected PHPT Paths

- `tests/phpt/generated/spl.file/file-classes-mvp.phpt`

## Target Gates

- `nix develop -c cargo test -p php_runtime resource`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=spl.file`

## Known Gaps

- `STDLIB-GAP-SPL-FILE-FULL-API`
- `STDLIB-GAP-SPL-FILE-CSV-FLAGS`

## Coverage

The selected fixture uses the PHPT source file and directory as allowed local
paths, verifies metadata methods, line reads, line iteration, and basic temp
file object metadata.
