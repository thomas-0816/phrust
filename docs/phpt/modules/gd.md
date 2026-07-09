# gd PHPT coverage

## Verified scope

- `gd` extension visibility.
- `GdImage` object surface for the selected bounded image fixture.
- `gd_info()` reports the currently supported backend capabilities without
  claiming unsupported formats.
- `imagecreatefromstring()`, `imagecreatefromjpeg()`,
  `imagecreatefrompng()`, and `imagecreatetruecolor()` for selected PNG/JPEG
  and truecolor paths.
- `imagesx()` and `imagesy()` dimension probes.
- `imagecopyresampled()` for the selected resize flow.
- `imagejpeg()` and `imagepng()` output for the selected fixture.
- `imagedestroy()` lifecycle behavior for the selected fixture.

## Known gaps

- Full libgd parity is not claimed.
- The broad GD drawing API, palette/truecolor edge cases, alpha/blending
  matrix, and font/text rendering remain future work.
- AVIF/WebP writing and format-specific option parity are outside the selected
  fixture.
- Imagick behavior is tracked separately and is not covered by the GD module
  manifest.
