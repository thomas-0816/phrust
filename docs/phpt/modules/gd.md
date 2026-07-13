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
- `imagecolorallocate()`, `imagecolorallocatealpha()`,
  `imagecolortransparent()`, `imagefill()`, `imagefilledrectangle()`,
  `imagerectangle()`, and `imageline()` for truecolor RGBA-backed drawing
  paths.
- `imagecopy()`, `imagecopyresized()`, `imagecopyresampled()`,
  `imagecopymerge()`, `imagescale()`, `imagerotate()`, and `imageflip()` for
  bounded bitmap copy and transform flows.
- `imagealphablending()` and `imagesavealpha()` option storage on `GdImage`
  objects.
- `imagejpeg()` and `imagepng()` output for the selected fixture.
- `imagedestroy()` lifecycle behavior for the selected fixture.

## libgd-gap report

- Full libgd parity is not claimed.
- Palette image behavior remains a known gap. The current backend stores
  `GdImage` as RGBA PNG bytes and returns PHP truecolor integer values, so
  palette allocation, palette exhaustion, closest/exact color search, and
  palette-to-truecolor conversion are not libgd-exact.
- Alpha blending is option-tracked but not libgd-exact across the full drawing
  matrix. Basic alpha values are preserved in RGBA pixels; blend-mode side
  effects and save-alpha interaction need libgd-level parity work before
  broader PHPT promotion.
- Font and text rendering remain out of scope for the `image` crate backend.
  `imagettftext()`, bitmap fonts, antialiasing, and FreeType-specific metrics
  need a dedicated font backend or libgd binding.
- GIF, WebP, AVIF, WBMP, GD/GD2, XBM, XPM, and TGA loaders/writers are not
  claimed unless the corresponding backend feature is explicitly enabled and
  covered by PHPT. `gd_info()` and `imagetypes()` therefore only advertise JPEG
  and PNG in this tree.
- Format-specific warning text, encoder options, interlace behavior, metadata,
  and byte-identical output remain known gaps. The selected fixtures assert
  successful object mutation, dimensions, and non-empty image output rather than
  byte-for-byte libgd encoding.
- Imagick behavior is tracked separately and is not covered by the GD module
  manifest.
