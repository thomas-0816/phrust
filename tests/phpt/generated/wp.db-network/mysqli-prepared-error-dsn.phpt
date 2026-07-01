--TEST--
wp.db-network: mysqli prepared statement error status DSN gate
--SKIPIF--
<?php
if (!extension_loaded("mysqli")) die("skip mysqli extension is not loaded");
if (getenv("PHRUST_MYSQL_TEST_DSN") === false || getenv("PHRUST_MYSQL_TEST_DSN") === "") die("skip PHRUST_MYSQL_TEST_DSN is not configured");
?>
--FILE--
<?php
$parts = parse_url(getenv("PHRUST_MYSQL_TEST_DSN"));
$host = $parts["host"] ?? "127.0.0.1";
$user = isset($parts["user"]) ? rawurldecode($parts["user"]) : "";
$pass = isset($parts["pass"]) ? rawurldecode($parts["pass"]) : "";
$dbName = isset($parts["path"]) ? ltrim($parts["path"], "/") : "";
$port = $parts["port"] ?? null;
$db = mysqli_connect($host, $user, $pass, $dbName, $port);
$stmt = mysqli_stmt_init($db);
var_dump(mysqli_stmt_prepare($stmt, "SELECT FROM"));
var_dump(mysqli_stmt_errno($stmt) > 0);
var_dump(mysqli_stmt_error($stmt) !== "");
var_dump(mysqli_stmt_sqlstate($stmt) !== "");
mysqli_stmt_close($stmt);
mysqli_close($db);
?>
--EXPECT--
bool(false)
bool(true)
bool(true)
bool(true)
