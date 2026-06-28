--TEST--
mysqli: platform checks stay negative
--DESCRIPTION--
Generated Branch 4 data-platform coverage for mysqli classification without network database support.
--FILE--
<?php
var_dump(extension_loaded("mysqli"));
var_dump(class_exists("mysqli", false));
var_dump(class_exists("mysqli_stmt", false));
var_dump(function_exists("mysqli_connect"));
?>
--EXPECT--
bool(false)
bool(false)
bool(false)
bool(false)
