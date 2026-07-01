--TEST--
wp.db-network: mysqli prepared bind_result and fetch DSN gate
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
$table = "phrust_stmt_bind_result_" . getmypid();
mysqli_query($db, "DROP TABLE IF EXISTS `$table`");
mysqli_query($db, "CREATE TABLE `$table` (id INT PRIMARY KEY, name VARCHAR(64))");
mysqli_query($db, "INSERT INTO `$table` (id, name) VALUES (7, 'seven')");
$stmt = mysqli_prepare($db, "SELECT id, name FROM `$table` WHERE id = ?");
$id = 7;
mysqli_stmt_bind_param($stmt, "i", $id);
mysqli_stmt_execute($stmt);
mysqli_stmt_bind_result($stmt, $outId, $outName);
var_dump(mysqli_stmt_fetch($stmt));
var_dump($outId);
var_dump($outName);
mysqli_stmt_free_result($stmt);
mysqli_stmt_close($stmt);
mysqli_query($db, "DROP TABLE IF EXISTS `$table`");
mysqli_close($db);
?>
--EXPECT--
bool(true)
int(7)
string(5) "seven"
