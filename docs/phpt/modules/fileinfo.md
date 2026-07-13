# fileinfo

- Strategy: libmagic-backed MIME detection for procedural and object Fileinfo APIs, with
  generated upload/media fixtures covering deterministic WordPress-facing
  paths and selected object flag behavior
- Selected manifest: `tests/phpt/manifests/modules/fileinfo.selected.jsonl`
- Selected fixtures: 38 rows
  - `tests/phpt/generated/fileinfo/mime-basic.phpt`
  - `tests/phpt/generated/fileinfo/svg.phpt`
  - `tests/phpt/generated/fileinfo/set-flags.phpt`
  - `ext/fileinfo/tests/finfo_file_basic.phpt`
  - `ext/fileinfo/tests/finfo_file_001.phpt`
  - `ext/fileinfo/tests/mime_content_type_001.phpt`
  - 15 upstream fileinfo object/regression rows promoted from
    `/private/tmp/phrust-phpt-fileinfo-full-target-finfo-object`
  - 8 upstream object-backed `finfo_open` rows promoted from
    `/private/tmp/phrust-phpt-fileinfo-full-target-finfo-open-object`
  - `ext/standard/tests/image/bug71848.phpt`
  - `ext/standard/tests/image/getimagesize_jpgapp.phpt`
  - `ext/standard/tests/image/getimagesize_variation2.phpt`
  - `ext/standard/tests/image/getimagesizefromstring1.phpt`
  - `ext/standard/tests/image/image_type_to_extension.phpt`
  - `ext/standard/tests/image/image_type_to_mime_type_basic.phpt`
  - `ext/standard/tests/image/image_type_to_mime_type_variation2.phpt`
  - `ext/standard/tests/image/image_type_to_mime_type_variation3.phpt`
  - `ext/standard/tests/image/image_type_to_mime_type_variation4.phpt`
- Current selected module gate:
  `/private/tmp/phrust-phpt-work/module-runs/fileinfo`
  reported reference PASS 8 / SKIP 30 and target PASS 37 / SKIP 1. The local
  php-src oracle binary was built without Fileinfo, so reference rows that
  require the extension skip while standard image-helper rows can still pass.
  The target skip is `ext/fileinfo/tests/cve-2014-3538-nojit.phpt`, which is
  not suitable for the debug PHPT build.
- Full upstream fileinfo target snapshot: 56 rows, PASS 24 / SKIP 1 /
  FAIL 31 at
  `/private/tmp/phrust-phpt-fileinfo-full-target-finfo-open-object`.
- Full image-helper target snapshot: 37 rows, PASS 9 / SKIP 3 / FAIL 25 at
  `/private/tmp/phrust-phpt-image-helper-full-target-app-info`.

## Implemented Surface

The runtime exposes `finfo_open`, `finfo_close`, `finfo_file`,
`finfo_buffer`, `finfo_set_flags`, `mime_content_type`,
`image_type_to_mime_type`, and `image_type_to_extension`.

`finfo_open` validates libmagic availability before returning a PHP 8.5
`finfo` object facade. `finfo_file` and `finfo_buffer` use libmagic as the
single MIME backend and store object flags across `finfo_set_flags`; phrust no
longer post-processes libmagic output with local MIME signature guesses.
Directory paths report `directory` before byte reads, matching the selected
PHP-visible behavior.

The VM recognizes the PHP 8.5 `finfo` class, supports `new finfo(...)`,
`instanceof finfo`, repeatable `$finfo->__construct()`, and dispatches
`$finfo->file()`, `$finfo->buffer()`, and `$finfo->set_flags()` through the same
runtime builtins as the procedural API.
Procedural `finfo_file`, `finfo_buffer`, and `finfo_set_flags` accept either the
legacy Fileinfo resource or the object facade as the first argument.

The stdlib descriptor exposes selected Fileinfo constants, including MIME mode
flags and common image type constants used by upload and media probes. The
image type helpers cover PHP's standard MIME and extension mappings for the
selected upstream rows.

`getimagesize` and `getimagesizefromstring` now return the selected PHP array
shape for GIF/JPEG/PNG/SVG/WebP rows: numeric width, height, image type, the
HTML dimension string, MIME, pixel units, and available bits/channels metadata.
`getimagesize` initializes the optional by-reference image-info argument to an
array and extracts JPEG APP segment payloads into `APPn` keys for selected
upstream rows.

## Gaps

This is not yet complete PHP 8.5 Fileinfo parity. Upstream rows that require
exact constructor diagnostics, clone/serialize lifecycle errors, or deprecation
text remain open.

The local libmagic backend uses the host or Nix `file` library. PHP's bundled
`ext/fileinfo/tests/magic` database is not currently load-compatible with that
host libmagic in this checkout, so upstream custom-magic PHPT rows are not
promoted yet.

The remaining full-target failures are dominated by host libmagic output
differences, custom magic database parsing/warning text, null-byte and path
validation diagnostics, deprecated context-parameter behavior, and finfo
clone/serialize/uninitialized lifecycle checks.

Image helper parity remains partial. BMP, TIFF, SWF/SWC, WBMP, XBM, ICO, JP2,
JPC, HEIF/AVIF `getimagesize` cases, corrupt-image warning text, IPTC helpers,
custom stream-wrapper rows, and broader SVG/WebP/AVIF PHPT coverage remain for
the Fileinfo/EXIF follow-up slice.

## Target Gates

- `nix develop -c cargo test -p php_runtime fileinfo`
- `nix develop -c cargo build -p php_vm_cli --bin php-vm`
- `nix develop -c just phpt-dev-module MODULE=fileinfo`
- `nix develop -c just phpt-dev-module MODULE=closure.extensions`
