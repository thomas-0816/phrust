--TEST--
wp.db-network: mysqli live query through DSN gate
--DESCRIPTION--
Prompt 3.3/3.4 live MySQL/MariaDB query contract.
--SKIPIF--
<?php
if (!extension_loaded("mysqli")) {
    die("skip mysqli extension is not loaded");
}
if (getenv("PHRUST_MYSQL_TEST_DSN") === false || getenv("PHRUST_MYSQL_TEST_DSN") === "") {
    die("skip PHRUST_MYSQL_TEST_DSN is not configured");
}
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
if ($db === false) {
    echo "connect failed\n";
    var_dump(mysqli_connect_errno() > 0);
    exit;
}
$result = mysqli_query($db, "SELECT 1 AS one");
var_dump($result instanceof mysqli_result);
var_dump(mysqli_num_rows($result));
var_dump(mysqli_fetch_assoc($result));
mysqli_free_result($result);
mysqli_close($db);
?>
--EXPECT--
bool(true)
int(1)
array(1) {
  ["one"]=>
  int(1)
}
