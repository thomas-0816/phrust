--TEST--
exif: JPEG image dimensions and minimal APP1 metadata
--DESCRIPTION--
Generated EXIF coverage for WordPress-style media metadata checks using a
small deterministic JPEG with TIFF/EXIF fields.
--SKIPIF--
<?php
if (!extension_loaded("exif")) die("skip exif extension not available");
?>
--FILE--
<?php
function le16($value) {
    return chr($value & 0xff) . chr(($value >> 8) & 0xff);
}
function le32($value) {
    return chr($value & 0xff) . chr(($value >> 8) & 0xff) . chr(($value >> 16) & 0xff) . chr(($value >> 24) & 0xff);
}
function be16($value) {
    return chr(($value >> 8) & 0xff) . chr($value & 0xff);
}
function exif_entry($tag, $type, $count, $value) {
    return le16($tag) . le16($type) . le32($count) . $value;
}
$dir = __DIR__ . "/exif-jpeg-basic";
$path = $dir . "/tiny.jpg";
@unlink($path);
@rmdir($dir);
mkdir($dir);
$ifd_count = 4;
$ifd_end = 8 + 2 + ($ifd_count * 12) + 4;
$date = "2026:06:28 12:00:00\0";
$tiff = "II" . le16(42) . le32(8);
$tiff .= le16($ifd_count);
$tiff .= exif_entry(0x0112, 3, 1, le16(6) . "\0\0");
$tiff .= exif_entry(0x010F, 2, 4, "PHP\0");
$tiff .= exif_entry(0x0110, 2, 4, "MVP\0");
$tiff .= exif_entry(0x0132, 2, strlen($date), le32($ifd_end));
$tiff .= le32(0) . $date;
$app1 = "Exif\0\0" . $tiff;
$sof0 = "\xff\xc0" . be16(17) . "\x08" . be16(3) . be16(2) . "\x03\x01\x11\x00\x02\x11\x00\x03\x11\x00";
$jpeg = "\xff\xd8" . "\xff\xe1" . be16(strlen($app1) + 2) . $app1 . $sof0 . "\xff\xd9";
file_put_contents($path, $jpeg);
var_dump(exif_imagetype($path));
$size = getimagesize($path);
echo $size[0], "x", $size[1], "|", $size[2], "|", $size["mime"], "\n";
$fromString = getimagesizefromstring($jpeg);
echo $fromString[0], "x", $fromString[1], "|", $fromString[2], "|", $fromString["mime"], "\n";
$data = exif_read_data($path);
echo $data["ImageWidth"], "x", $data["ImageLength"], "|", $data["Orientation"], "|", $data["Make"], "|", $data["Model"], "|", $data["DateTime"], "\n";
$alias = read_exif_data($path);
echo $alias["Orientation"], "|", $alias["Make"], "|", $alias["Model"], "\n";
?>
--CLEAN--
<?php
$dir = __DIR__ . "/exif-jpeg-basic";
@unlink($dir . "/tiny.jpg");
@rmdir($dir);
?>
--EXPECT--
int(2)
2x3|2|image/jpeg
2x3|2|image/jpeg
2x3|6|PHP|MVP|2026:06:28 12:00:00
6|PHP|MVP
