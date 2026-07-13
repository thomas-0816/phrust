--TEST--
gd: common drawing, copy, and transform helpers
--EXTENSIONS--
gd
--SKIPIF--
<?php
if (!extension_loaded("gd")) die("skip gd extension not available");
?>
--FILE--
<?php
$dir = __DIR__ . "/gd-drawing-transforms";
@mkdir($dir);
$png = $dir . "/out.png";

$img = imagecreatetruecolor(4, 3);
var_dump($img instanceof GdImage);
$red = imagecolorallocate($img, 255, 0, 0);
$blue = imagecolorallocatealpha($img, 0, 0, 255, 0);
echo $red, "\n";
echo $blue, "\n";
var_dump(imagefill($img, 0, 0, $red));
var_dump(imagefilledrectangle($img, 1, 1, 2, 2, $blue));
var_dump(imageline($img, 0, 0, 3, 2, $blue));
var_dump(imagerectangle($img, 0, 0, 3, 2, $red));
var_dump(imagealphablending($img, false));
var_dump(imagesavealpha($img, true));
echo imagecolortransparent($img), "\n";
echo imagecolortransparent($img, $blue), "\n";

$copy = imagecreatetruecolor(4, 3);
var_dump(imagecopy($copy, $img, 0, 0, 0, 0, 4, 3));
var_dump(imagecopymerge($copy, $img, 0, 0, 0, 0, 4, 3, 50));
var_dump(imagecopyresized($copy, $img, 0, 0, 0, 0, 2, 2, 4, 3));

$scaled = imagescale($img, 8, -1);
var_dump($scaled instanceof GdImage);
echo imagesx($scaled), "x", imagesy($scaled), "\n";
$rotated = imagerotate($img, 90, $blue);
var_dump($rotated instanceof GdImage);
echo imagesx($rotated), "x", imagesy($rotated), "\n";
var_dump(imageflip($img, 1));
var_dump(imagepng($scaled, $png));
var_dump(file_exists($png));
var_dump(filesize($png) > 0);
?>
--CLEAN--
<?php
$dir = __DIR__ . "/gd-drawing-transforms";
@unlink($dir . "/out.png");
@rmdir($dir);
?>
--EXPECT--
bool(true)
16711680
255
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
-1
255
bool(true)
bool(true)
bool(true)
bool(true)
8x6
bool(true)
3x4
bool(true)
bool(true)
bool(true)
bool(true)
