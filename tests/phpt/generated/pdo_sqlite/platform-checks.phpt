--TEST--
pdo_sqlite: platform checks stay negative
--DESCRIPTION--
Generated Branch 4 data-platform coverage for PDO SQLite classification without fake query behavior.
--FILE--
<?php
var_dump(extension_loaded("pdo_sqlite"));
var_dump(extension_loaded("pdo"));
var_dump(class_exists("PDO", false));
var_dump(class_exists("PDOStatement", false));
?>
--EXPECT--
bool(false)
bool(false)
bool(false)
bool(false)
