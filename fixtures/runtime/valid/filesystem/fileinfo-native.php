<?php

$finfo = finfo_open(FILEINFO_MIME_TYPE);
echo get_class($finfo), "\n";
echo finfo_buffer($finfo, "<?php echo 1;"), "\n";
echo finfo_file($finfo, __FILE__), "\n";
var_dump(finfo_set_flags($finfo, FILEINFO_MIME));
var_dump(finfo_close($finfo));

$image = sys_get_temp_dir() . "/phrust-fileinfo-native.png";
file_put_contents($image, "\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x02\x00\x00\x00\x03");
$image_type = exif_imagetype($image);
echo $image_type, "\n";
echo image_type_to_mime_type($image_type), "\n";
$app = ['stale'];
$size = getimagesize($image, $app);
echo $size[0], "x", $size[1], "|", $size[2], "|", $size['mime'], "|", count($app), "\n";
unlink($image);
