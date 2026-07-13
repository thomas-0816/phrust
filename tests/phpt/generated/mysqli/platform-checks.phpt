--TEST--
mysqli: platform checks expose WordPress MVP
--DESCRIPTION--
WordPress DB/network branch coverage for the mysqli MVP visibility contract.
--SKIPIF--
<?php
if (!extension_loaded("mysqli")) {
    die("skip mysqli extension is not loaded");
}
?>
--FILE--
<?php
var_dump(extension_loaded("mysqli"));
var_dump(class_exists("mysqli", false));
var_dump(class_exists("mysqli_result", false));
var_dump(class_exists("mysqli_stmt", false));
var_dump(function_exists("mysqli_connect"));
var_dump(function_exists("mysqli_autocommit"));
var_dump(function_exists("mysqli_begin_transaction"));
var_dump(function_exists("mysqli_commit"));
var_dump(function_exists("mysqli_get_client_stats"));
var_dump(function_exists("mysqli_get_connection_stats"));
var_dump(function_exists("mysqli_multi_query"));
var_dump(function_exists("mysqli_next_result"));
var_dump(function_exists("mysqli_ping"));
var_dump(function_exists("mysqli_query"));
var_dump(function_exists("mysqli_rollback"));
var_dump(function_exists("mysqli_store_result"));
var_dump(function_exists("mysqli_prepare"));
var_dump(function_exists("mysqli_stmt_result_metadata"));
var_dump(function_exists("mysqli_use_result"));
var_dump(defined("MYSQLI_ASSOC"));
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
