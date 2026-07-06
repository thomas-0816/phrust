# imagick

- Priority: ImageMagick-backed media extension
- Selected manifest: `tests/phpt/manifests/modules/imagick.selected.jsonl`
- Current focused snapshot: 1 target-side PASS covering the registered
  extension/class surface and fail-closed backend gate

## Scope

- `extension_loaded("imagick")`
- Internal class metadata for `Imagick`, `ImagickDraw`, `ImagickPixel`,
  `ImagickPixelIterator`, and `ImagickException`
- Reflection class ownership for the PECL Imagick class surface
- `Imagick` method metadata for the first WordPress-relevant image editor
  selection probes
- Deterministic constructor failure until an explicit ImageMagick backend is
  wired

## Non-Scope

- ImageMagick/MagickWand binding or FFI execution
- Read/write/resize/crop/thumbnail/image-identification behavior
- Image formats, profiles, EXIF/IPTC metadata, colorspaces, layers, animations,
  resource limits, policy files, and exception parity
- Imagick constants with ImageMagick-version-dependent numeric values

## Selected PHPT Fixtures

- `tests/phpt/generated/imagick/backend-gate.phpt`

## Relevant Source Areas

- `crates/php_std/src/extensions.rs`
- `crates/php_std/src/lib.rs`
- `crates/php_vm/src/vm/mod.rs`

## Oracle Notes

- The pinned PHP 8.5.7 php-src checkout has no `ext/imagick/` tree.
- The local reference CLI does not load `imagick`, so selected generated
  coverage is target-side policy coverage until a PECL Imagick oracle is
  available.

## Target Gates

- `nix develop -c cargo test -p php_std imagick --no-fail-fast`
- `nix develop -c cargo test -p php_vm imagick --no-fail-fast`
- `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=imagick`

## Known Gaps

- `IMAGICK-1` still requires a real ImageMagick backend, filesystem integration,
  constants, and basic image operations.
- `IMAGICK-2` still requires advanced filters, formats, profiles, metadata,
  colorspaces, layers, animations, resource limits, policy handling, and oracle
  metadata/dimension comparison.
