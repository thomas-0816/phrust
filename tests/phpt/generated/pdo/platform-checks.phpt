--TEST--
pdo: platform checks stay negative
--DESCRIPTION--
Generated Branch 4 data-platform coverage for PDO classification without enabling fake database support.
--FILE--
<?php
var_dump(extension_loaded("pdo"));
var_dump(class_exists("PDO", false));
var_dump(class_exists("PDOException", false));
var_dump(class_exists("PDOStatement", false));
?>
--EXPECT--
bool(false)
bool(false)
bool(false)
bool(false)
