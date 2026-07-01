--TEST--
wp.db-network: mysqli prepared statement reexecute reads bound values
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
$table = "phrust_stmt_reexecute_" . getmypid();
mysqli_query($db, "DROP TABLE IF EXISTS `$table`");
mysqli_query($db, "CREATE TABLE `$table` (id INT PRIMARY KEY, name VARCHAR(64))");
$stmt = mysqli_prepare($db, "INSERT INTO `$table` (id, name) VALUES (?, ?)");
$id = 1;
$name = "alpha";
mysqli_stmt_bind_param($stmt, "is", $id, $name);
var_dump(mysqli_stmt_execute($stmt));
$id = 2;
$name = "beta";
var_dump(mysqli_stmt_execute($stmt));
mysqli_stmt_close($stmt);
$result = mysqli_query($db, "SELECT name FROM `$table` ORDER BY id");
var_dump(mysqli_fetch_row($result));
var_dump(mysqli_fetch_row($result));
mysqli_free_result($result);
mysqli_query($db, "DROP TABLE IF EXISTS `$table`");
mysqli_close($db);
?>
--EXPECT--
bool(true)
bool(true)
array(1) {
  [0]=>
  string(5) "alpha"
}
array(1) {
  [0]=>
  string(4) "beta"
}
