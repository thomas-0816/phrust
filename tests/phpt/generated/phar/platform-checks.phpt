--TEST--
phar: platform checks stay negative
--DESCRIPTION--
Generated Branch 4 data-platform coverage for PHAR classification without a read-only phar:// MVP.
--FILE--
<?php
var_dump(extension_loaded("phar"));
var_dump(class_exists("Phar", false));
var_dump(class_exists("PharData", false));
var_dump(class_exists("PharFileInfo", false));
?>
--EXPECT--
bool(false)
bool(false)
bool(false)
bool(false)
