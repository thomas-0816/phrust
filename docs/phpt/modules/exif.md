# exif

- Strategy: keep WordPress-facing image probes deterministic while expanding
  selected upstream EXIF PHPT coverage in small, promoted slices.
- Selected manifest: `tests/phpt/manifests/modules/exif.selected.jsonl`
- Selected fixtures:
  - `tests/phpt/generated/exif/jpeg-metadata-basic.phpt`
  - `ext/exif/tests/bug50660/bug50660.phpt`
  - `ext/exif/tests/bug71534.phpt`
  - `ext/exif/tests/bug72735/bug72735.phpt`
  - `ext/exif/tests/bug72819/bug72819.phpt`
  - `ext/exif/tests/bug76130.phpt`
  - `ext/exif/tests/bug76164.phpt`
  - `ext/exif/tests/bug77753.phpt`
  - `ext/exif/tests/bug77988.phpt`
  - `ext/exif/tests/bug78222.phpt`
  - `ext/exif/tests/bug78256.phpt`
  - `ext/exif/tests/bug78793.phpt`
  - `ext/exif/tests/duplicate_copyright_tag_leak.phpt`
  - `ext/exif/tests/exif_encoding_crash.phpt`
  - `ext/exif/tests/exif_imagetype_basic-mb.phpt`
  - `ext/exif/tests/exif_imagetype_basic.phpt`
  - `ext/exif/tests/exif_imagetype_error.phpt`
  - `ext/exif/tests/exif_tagname_basic.phpt`
  - `ext/exif/tests/filename_empty.phpt`
  - `ext/exif/tests/exif_thumbnail_streams.phpt`
  - `ext/exif/tests/nesting_level_oom.phpt`
  - `ext/exif/tests/redhat-bug1362571.phpt`
  - `ext/exif/tests/temporary_buffer_leak.phpt`
  - `ext/exif/tests/zero_length_makernote_leak.phpt`
- Current selected module gate:
  `/private/tmp/phrust-phpt-exif-selected-after-rebase`
  reported reference SKIP 24 / target PASS 24. The local php-src oracle binary
  was built without EXIF, so reference rows skip while target rows prove the
  promoted phrust behavior.
- Last full upstream target sweep:
  `/private/tmp/phrust-phpt-exif-full-target-image-helpers`
  reported 93 upstream rows: PASS 22 / SKIP 1 / FAIL 70.

## Implemented Surface

The runtime exposes `exif_imagetype`, `exif_read_data`, `exif_tagname`,
`exif_thumbnail`, `getimagesize`, and `getimagesizefromstring`.

`exif_read_data` currently reports common JPEG/TIFF fields needed by the
selected media fixture: image dimensions, orientation, make, model, and
DateTime. `exif_tagname` covers selected common IFD tag names, including the
upstream basic row. `exif_thumbnail` reads JPEG EXIF APP1/TIFF IFD1 thumbnail
offset and length tags, supports stream resources, preserves seekable stream
cursors across repeated calls, and fills optional width, height, and image type
reference arguments. Empty filenames and filenames containing null bytes raise
the selected PHP `ValueError` messages.

The VM named-argument binder supports `exif_thumbnail` and skipped optional
by-reference parameters, which is required for calls such as
`exif_thumbnail($stream, height: $height)`.

The selected upstream set also covers target-green EXIF parser stability rows:
two illegal-IFD-offset JPEGs, several malformed TIFF/JPEG crash/leak
regressions, multibyte `exif_imagetype` filenames, and representative
thumbnail extraction regressions.

## Gaps

This is not complete PHP 8.5 EXIF parity. The implementation still uses a
bounded internal TIFF reader for selected tags and thumbnail extraction; it has
not yet been replaced with a mature EXIF/TIFF crate or libexif bridge.

MakerNote parsing, GPS and interoperability tag matrices, full TIFF variants,
HEIF EXIF metadata, data-wrapper inputs for `exif_read_data`, FILE/COMPUTED/IFD
section shape parity, additional thumbnail bug regression rows, and exact
corrupt-image warning text remain unpromoted. Broader upstream rows such as the
`exif00x` fixtures should be promoted only after the parser backend is widened.

## Target Gates

- `nix develop -c cargo test -p php_runtime exif`
- `nix develop -c cargo build -p php_vm_cli --bin phrust-php`
- `nix develop -c just phpt-dev-module MODULE=exif`
- `nix develop -c just verify-stdlib`
