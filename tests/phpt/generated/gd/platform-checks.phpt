--TEST--
gd: platform checks stay negative
--DESCRIPTION--
Generated Branch 4 data-platform coverage for GD classification without image processing.
--FILE--
<?php
var_dump(extension_loaded("gd"));
var_dump(class_exists("GdImage", false));
var_dump(function_exists("imagecreatetruecolor"));
var_dump(function_exists("imagepng"));
?>
--EXPECT--
bool(false)
bool(false)
bool(false)
bool(false)
