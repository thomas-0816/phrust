# phar PHPT Coverage

## Strategy

The phar slice is a read-only MVP for WordPress and Composer-adjacent archive
visibility. It promotes generated platform fixtures plus upstream rows that are
green without requiring writable archive mutation, signature enforcement, or
full `phar://` directory/stat behavior.

## Selected Rows

- `tests/phpt/generated/phar/platform-checks.phpt`
- `tests/phpt/generated/phar/read-only-methods.phpt`
- `ext/phar/tests/phar_get_supportedcomp1.phpt`
- `ext/phar/tests/phar_get_supportedcomp2.phpt`
- `ext/phar/tests/phar_get_supportedcomp3.phpt`
- `ext/phar/tests/phar_get_supportedcomp4.phpt`
- `ext/phar/tests/phar_get_supported_signatures_002.phpt`
- `ext/phar/tests/phar_get_supported_signatures_002a.phpt`
- `ext/phar/tests/bug66960.phpt`
- `ext/phar/tests/bug74383.phpt`
- `ext/phar/tests/bug74386.phpt`
- `ext/phar/tests/bug79797.phpt`

## Implemented Surface

- `extension_loaded('phar')`, `class_exists` visibility for `Phar`,
  `PharData`, and `PharFileInfo`, and selected `Phar` object construction.
- Read-only local `.phar` manifest parsing, `phar://` file reads, and
  `include`/`require` loading for uncompressed archive entries.
- Selected read-only `Phar` methods including path, alias, stub, count, and
  `offsetExists` behavior over local uncompressed archives.
- Static supported compression/signature capability probes, including expected
  target skips for unavailable `bz2`/OpenSSL capability combinations.
- Long `phar://` path existence checks and no-crash `phar.cache_list`
  initialization coverage.
- Reflection arginfo for selected `Phar::running` and `Phar::__construct`
  surfaces.

## Current Gate

The selected phar module gate is policy-green with 12 selected rows. In the
current local php-src oracle build, reference rows skip because the phar
extension is not loaded. The target runtime reports 8 PASS and 4 SKIP; the
target skips are capability-gated upstream rows for unavailable `bz2` or
OpenSSL combinations.

```text
REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php \
PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src \
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_DISABLE_REFERENCE_REUSE=1 \
PHPT_TIMEOUT_SECONDS=20 \
PHPT_WORK_DIR=/private/tmp/phrust-phpt-phar-selected-expanded \
nix develop -c just phpt-dev-module MODULE=phar
```

A temporary target-only upstream sweep was also run from a generated manifest
outside the repository:

```text
target/debug/php-phpt-tools run \
  --manifest /private/tmp/phrust-phar-manifest-current/phar-originals.jsonl \
  --out /private/tmp/phrust-phpt-phar-originals-current/results.jsonl
```

That sweep reported 565 upstream phar rows: PASS 6 / SKIP 38 / FAIL 381 /
BORK 140. Four target-green upstream rows not already selected were promoted.

## Remaining Gaps

- Writable `Phar` creation/update APIs, `phar.readonly` write errors, archive
  mutation, `buildFromDirectory`, `buildFromIterator`, and `convertTo*` remain
  unpromoted.
- `PharData` tar/zip objects and `PharFileInfo` iteration/object construction
  are not implemented in the runtime MVP.
- Compressed archive reads/writes, signature validation/enforcement, metadata
  unserialization, and full stub execution remain outside the selected gate.
- Full `phar://` directory iteration, stat behavior, aliases, autoloading, and
  Composer phar execution remain known gaps.
