--TEST--
sqlite3: platform checks stay negative
--DESCRIPTION--
Generated Branch 4 data-platform coverage for SQLite3 classification without an in-memory MVP.
--FILE--
<?php
var_dump(extension_loaded("sqlite3"));
var_dump(class_exists("SQLite3", false));
var_dump(class_exists("SQLite3Stmt", false));
var_dump(class_exists("SQLite3Result", false));
?>
--EXPECT--
bool(false)
bool(false)
bool(false)
bool(false)
