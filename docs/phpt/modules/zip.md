# zip PHPT Coverage

## Strategy

The zip slice focuses on WordPress-facing archive inspection, extraction, and
basic archive creation paths. Selected rows combine deterministic generated
fixtures with upstream php-src object and legacy-resource PHPTs.

## Selected Rows

- `tests/phpt/generated/zip/archive-basic.phpt`
- `tests/phpt/generated/zip/legacy-resource-api.phpt`
- `ext/zip/tests/zip_open.phpt`
- `ext/zip/tests/zip_open_error.phpt`
- `ext/zip/tests/zip_close.phpt`
- `ext/zip/tests/zip_read.phpt`
- `ext/zip/tests/zip_entry_name.phpt`
- `ext/zip/tests/zip_entry_filesize.phpt`
- `ext/zip/tests/zip_entry_compressedsize.phpt`
- `ext/zip/tests/zip_entry_compressionmethod.phpt`
- `ext/zip/tests/zip_entry_open.phpt`
- `ext/zip/tests/zip_entry_read.phpt`
- `ext/zip/tests/zip_entry_close.phpt`
- `ext/zip/tests/oo_open.phpt`
- `ext/zip/tests/oo_extract.phpt`
- `ext/zip/tests/bug40228.phpt`
- `ext/zip/tests/oo_count.phpt`
- `ext/zip/tests/oo_getnameindex.phpt`
- `ext/zip/tests/oo_namelocate.phpt`
- `ext/zip/tests/bug53885.phpt`
- `ext/zip/tests/oo_supported.phpt`
- `ext/zip/tests/oo_addemptydir.phpt`
- `ext/zip/tests/oo_addemptydir_error.phpt`
- `ext/zip/tests/oo_addfile.phpt`
- `ext/zip/tests/oo_add_from_string.phpt`
- `ext/zip/tests/oo_delete.phpt`
- `ext/zip/tests/oo_rename.phpt`
- `ext/zip/tests/oo_setcomment.phpt`
- `ext/zip/tests/bug38944.phpt`
- `ext/zip/tests/oo_getcomment.phpt`
- `ext/zip/tests/oo_close.phpt`
- `ext/zip/tests/oo_setcomment_error.phpt`
- `ext/zip/tests/oo_properties.phpt`
- `ext/zip/tests/oo_archive_flag.phpt`
- `ext/zip/tests/oo_close_empty.phpt`
- `ext/zip/tests/doubleclose.phpt`
- `ext/zip/tests/001.phpt`
- `ext/zip/tests/bug11216.phpt`
- `ext/zip/tests/bug14962.phpt`
- `ext/zip/tests/bug40228-mb.phpt`
- `ext/zip/tests/bug47667.phpt`
- `ext/zip/tests/bug7214.phpt`
- `ext/zip/tests/bug7658.phpt`
- `ext/zip/tests/bug77978.phpt`
- `ext/zip/tests/bug8009.phpt`
- `ext/zip/tests/bug80863.phpt`
- `ext/zip/tests/bug8700.phpt`
- `ext/zip/tests/bug_gh8781.phpt`
- `ext/zip/tests/oo_ext_zip.phpt`
- `ext/zip/tests/pecl12414.phpt`

## Implemented Surface

- `ZipArchive::open`, `close`, `count`, `getFromName`, `getFromIndex`,
  `getNameIndex`, `locateName`, `statName`, `statIndex`, and `extractTo`,
  including recursive empty-directory extraction from selected upstream
  archives.
- `ZipArchive::open` reports selected missing-file false, `CREATE`, and
  empty-filename `ValueError` behavior, plus the PHP 8.5 deprecation for
  opening an existing zero-byte file as an archive.
- `ZipArchive::close` reports the selected invalid/uninitialized object
  `ValueError` for double-close behavior and selected empty-archive close
  delete/keep behavior.
- `ZipArchive` implements `Countable`; `count($zip)` reflects `numFiles` for
  opened archives.
- `ZipArchive::addFile`, `addFromString`, and `addEmptyDir` for new archives
  and selected existing-archive append/overwrite flows, including `lastId`,
  `numFiles`, and duplicate `ER_EXISTS` status behavior.
- Duplicate `addEmptyDir`, duplicate `addFromString`, and `OVERWRITE`
  replacement rows cover repeated-entry behavior and archive truncation
  semantics.
- `ZipArchive::deleteIndex`, `deleteName`, `renameIndex`, and `renameName`
  for selected existing-archive mutation flows. `ZipArchive::open` with
  `CREATE` now preserves existing archive entries unless `OVERWRITE` is set.
- `ZipArchive::open` with `RDONLY`, `getArchiveFlag`, and `setArchiveFlag`
  cover the selected read-only and empty-archive keep flags. Mutating selected
  archive entries in read-only mode returns `false` and sets `ER_RDONLY`.
- Selected extraction rows cover omitted/null file lists, reference-array file
  lists, recursive empty directories with multibyte archive names, and POSIX
  directory names ending in colons.
- Selected damaged/general-bit archive rows cover `getFromName`, `statName`,
  `statIndex`, and mutation of archives with general bit flag 3 set.
- `ZipArchive::setArchiveComment`, `getArchiveComment`, `setCommentIndex`,
  `setCommentName`, `getCommentIndex`, and `getCommentName` for selected
  archive creation, close, reopen, persistent comment round-trips, and
  catchable empty-name `ValueError` behavior.
- Oversized archive and entry comments throw catchable `ValueError` with the
  selected PHP argument labels.
- `ZipArchive` exposes the selected public property/debug shape for opened and
  newly created archives while hiding internal runtime bookkeeping from
  `var_dump`, and undefined-property warnings preserve the PHP-visible class
  display name.
- `ZipArchive::isCompressionMethodSupported` and
  `ZipArchive::isEncryptionMethodSupported` with deterministic support
  reporting for the selected PHP-visible constants.
- `ZipArchive::locateName` honors `FL_NOCASE`, `FL_NODIR`, and the selected
  `FL_UNCHANGED` constant path for upstream name lookup behavior.
- Legacy resource helpers: `zip_open`, `zip_read`, `zip_close`,
  `zip_entry_open`, `zip_entry_read`, `zip_entry_close`, `zip_entry_name`,
  `zip_entry_filesize`, `zip_entry_compressedsize`, and
  `zip_entry_compressionmethod`, including selected PHP 8 deprecation notices
  and selected invalid-resource `TypeError` behavior for `zip_close()` and
  `zip_entry_close()`, and binary-safe `zip_entry_read()`.
- `zip_open()` reports the selected empty-filename `ValueError` after its PHP
  8 deprecation notice and returns non-resource failure for a missing archive
  path.
- `extension_loaded('zip')` and `new ZipArchive` subclass construction/property
  behavior are covered by selected upstream rows.

## Current Gate

The selected zip module gate is policy-green with 50 selected rows. In the
current local php-src oracle build, reference rows skip because the zip
extension is not loaded; the target runtime reports 50 PASS and 0 non-green
outcomes.

```text
REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php \
PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src \
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_TIMEOUT_SECONDS=20 \
PHPT_WORK_DIR=/private/tmp/phrust-phpt-zip-selected-expanded-after-zlib \
nix develop -c just phpt-dev-module MODULE=zip
```

A temporary target-only originals sweep was also run from a generated manifest
outside the repository:

```text
target/debug/php-phpt-tools run \
  --manifest /private/tmp/phrust-zip-manifest-after-zlib/zip-originals-no-bug72258.jsonl \
  --out /private/tmp/phrust-phpt-zip-originals-after-zlib/results.jsonl
```

That sweep reported 107 upstream zip originals: PASS 48 / SKIP 2 / FAIL 57.
Fourteen newly target-green upstream rows not already selected were promoted.
The temporary sweep excluded `ext/zip/tests/bug72258.phpt` because the upstream
PHPT source contains non-UTF8 bytes that the current PHPT generator cannot
parse.

## Remaining Gaps

- Password/encryption read/write behavior and encrypted-entry diagnostics
  remain outside the current runtime surface.
- `ZipArchive::addGlob`, `ZipArchive::addPattern`, `ZipArchive::getStream`,
  `zip://` stream wrapper behavior, zip64 parity, external attributes,
  progress/cancel callbacks, and full `status`/`statusSys` parity remain
  unpromoted.
- The PHPT generator cannot currently parse the non-UTF8 upstream
  `ext/zip/tests/bug72258.phpt` source.
- `ext/zip/tests/bug76524.phpt` currently matches the target output (`ok`) but
  is not promoted because the PHPT runner cannot parse its upstream
  negative-lookahead `EXPECTREGEX` pattern.
- The full upstream zip corpus has 108 PHPT rows; this selected gate promotes
  the subset currently proven against the local oracle and target.
