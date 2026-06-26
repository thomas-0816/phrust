# extension.policy

- Priority: 22
- Selected manifest: `tests/phpt/manifests/modules/extension.policy.selected.jsonl`
- Current counts: 468 PASS, 525 SKIP, 7757 FAIL, 0 BORK from 9006 corpus candidates

## Scope

- non-core extension classification
- must-fix vs optional/out-of-scope routing

## Non-Scope

- large extension implementation

## Relevant PHPT Paths

- `tests/security/open_basedir_unlink.phpt`
- `tests/security/open_basedir_touch.phpt`
- `tests/security/open_basedir_tempnam.phpt`
- `tests/security/open_basedir_symlink.phpt`
- `tests/security/open_basedir_stat.phpt`
- `tests/security/open_basedir_scandir.phpt`
- `tests/security/open_basedir_rmdir.phpt`
- `tests/security/open_basedir_rename.phpt`
- `tests/security/open_basedir_realpath.phpt`
- `tests/security/open_basedir_readlink.phpt`
- `tests/security/open_basedir_parse_ini_file.phpt`
- `tests/security/open_basedir_opendir.phpt`
- `tests/security/open_basedir_mkdir.phpt`
- `tests/security/open_basedir_lstat.phpt`
- `tests/security/open_basedir_linkinfo.phpt`
- `tests/security/open_basedir_link.phpt`
- `tests/security/open_basedir_is_writable.phpt`
- `tests/security/open_basedir_is_readable.phpt`
- `tests/security/open_basedir_is_link.phpt`
- `tests/security/open_basedir_is_file.phpt`
- `tests/security/open_basedir_is_executable.phpt`
- `tests/security/open_basedir_is_dir.phpt`
- `tests/security/open_basedir_glob_variation.phpt`
- `tests/security/open_basedir_glob.phpt`
- `tests/security/open_basedir_fopen.phpt`
- `tests/security/open_basedir_filetype.phpt`
- `tests/security/open_basedir_filesize.phpt`
- `tests/security/open_basedir_fileperms.phpt`
- `tests/security/open_basedir_fileowner.phpt`
- `tests/security/open_basedir_filemtime.phpt`
- `tests/security/open_basedir_fileinode.phpt`
- `tests/security/open_basedir_filegroup.phpt`
- `tests/security/open_basedir_filectime.phpt`
- `tests/security/open_basedir_fileatime.phpt`
- `tests/security/open_basedir_file.phpt`
- `tests/security/open_basedir_error_log_variation.phpt`
- `tests/security/open_basedir_error_log.phpt`
- `tests/security/open_basedir_disk_free_space.phpt`
- `tests/security/open_basedir_dir.phpt`
- `tests/security/open_basedir_copy_variation1.phpt`

## Relevant php-src Source Areas

- `ext/dom/`
- `ext/xml/`
- `ext/soap/`
- `ext/intl/`
- `ext/gd/`
- `ext/opcache/`

## Target Gates

- `nix develop -c just phpt-triage`

## Known Gaps

- `runtime-error-or-diagnostic`: 5466
- `runtime-unsupported-feature`: 2431
- `runtime-output-mismatch`: 450
- `frontend-parse-or-compile`: 89
- `runtime-timeout`: 2

## Next Step

Classify extension failures without hiding them from full regression.
