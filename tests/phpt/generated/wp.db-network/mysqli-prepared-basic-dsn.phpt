--TEST--
wp.db-network: mysqli prepared statement basic DSN gate
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
$table = "phrust_stmt_basic_" . getmypid();
mysqli_query($db, "DROP TABLE IF EXISTS `$table`");
mysqli_query($db, "CREATE TABLE `$table` (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(64))");
$stmt = mysqli_prepare($db, "INSERT INTO `$table` (name) VALUES (?)");
$name = "alpha";
var_dump($stmt instanceof mysqli_stmt);
var_dump(mysqli_stmt_bind_param($stmt, "s", $name));
var_dump(mysqli_stmt_execute($stmt));
var_dump(mysqli_stmt_affected_rows($stmt) >= 0);
mysqli_stmt_close($stmt);
$result = mysqli_query($db, "SELECT name FROM `$table` ORDER BY id");
var_dump(mysqli_fetch_assoc($result));
mysqli_free_result($result);
mysqli_query($db, "DROP TABLE IF EXISTS `$table`");
mysqli_close($db);
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
array(1) {
  ["name"]=>
  string(5) "alpha"
}
