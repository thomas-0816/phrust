# imagick PHPT coverage

## Verified scope

- `imagick` extension visibility.
- `Imagick` class surface metadata.
- Reflection metadata for extension and internal class visibility.
- WordPress image-editor selection method probes.
- Fail-closed constructor behavior when no ImageMagick backend is available.

## Known gaps

- ImageMagick or MagickWand binding/FFI execution is not implemented by the
  selected fixture.
- Read, write, resize, crop, thumbnail, and identify operations remain future
  work.
- Formats, profiles, EXIF/IPTC metadata, colorspaces, layers, animations,
  resource limits, and policy-file behavior are not claimed.
- ImageMagick-version-dependent constants remain outside the current bounded
  manifest.
