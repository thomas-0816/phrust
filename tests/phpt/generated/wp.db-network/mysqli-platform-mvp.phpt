--TEST--
wp.db-network: mysqli MVP platform visibility
--DESCRIPTION--
Prompt 3.3 coverage for WordPress DB/network branch startup. Binaries without a
native or target mysqli module skip cleanly.
--SKIPIF--
<?php
if (!extension_loaded("mysqli")) {
    die("skip mysqli extension is not loaded");
}
?>
--FILE--
<?php
var_dump(extension_loaded("mysqli"));
var_dump(extension_loaded("mysqlnd"));
var_dump(class_exists("mysqli", false));
var_dump(class_exists("mysqli_result", false));
var_dump(class_exists("mysqli_stmt", false));
var_dump(function_exists("mysqli_connect"));
var_dump(function_exists("mysqli_query"));
var_dump(defined("MYSQLI_ASSOC"));
var_dump(MYSQLI_ASSOC);
var_dump(MYSQLI_NUM);
var_dump(MYSQLI_BOTH);
?>
--EXPECT--
bool(true)
bool(false)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
int(1)
int(2)
int(3)
