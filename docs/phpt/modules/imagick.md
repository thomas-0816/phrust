# imagick PHPT coverage

## Verified scope

- `imagick` extension visibility.
- `Imagick` class surface metadata.
- Reflection metadata for extension and internal class visibility.
- WordPress image-editor selection method probes.
- Common image-operation method probes on `Imagick`: constructor, read/write,
  blob read/write, resize, thumbnail, crop, dimension, format, strip, and
  identify names are visible for app capability checks.
- Fail-closed constructor behavior when no ImageMagick backend is available.

## Known gaps

- ImageMagick or MagickWand binding/FFI execution is not implemented by the
  selected fixture.
- Constructor, read, write, blob, resize, crop, thumbnail, dimension, format,
  strip, and identify operations remain backend-gated future work. Calls must
  fail with `E_PHP_VM_UNSUPPORTED_IMAGICK` unless a real ImageMagick backend is
  wired; they must not return fake successful image results.
- Formats, profiles, EXIF/IPTC metadata, colorspaces, layers, animations,
  resource limits, and policy-file behavior are not claimed.
- ImageMagick-version-dependent constants remain outside the current bounded
  manifest.
