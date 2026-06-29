--TEST--
wp.db-network: mysqli object wpdb-style live query through DSN
--DESCRIPTION--
live WordPress wpdb shape: connect, set utf8mb4, create a temporary
table, insert an escaped value, select it back, and fetch through mysqli_result.
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

$db = mysqli_init();
var_dump($db instanceof mysqli);
var_dump($db->real_connect($host, $user, $pass, $dbName, $port));
var_dump($db->set_charset("utf8mb4"));

$raw = "wp'db";
$escaped = $db->real_escape_string($raw);
var_dump($escaped !== $raw);

var_dump($db->query("CREATE TEMPORARY TABLE phrust_wpdb_mvp (id INT PRIMARY KEY AUTO_INCREMENT, value VARCHAR(255))"));
var_dump($db->query("INSERT INTO phrust_wpdb_mvp (value) VALUES ('$escaped')"));
$result = $db->query("SELECT value FROM phrust_wpdb_mvp ORDER BY id");
var_dump($result instanceof mysqli_result);
var_dump($result->num_rows);
var_dump($result->fetch_assoc());
var_dump($result->free());
var_dump($db->close());
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
int(1)
array(1) {
  ["value"]=>
  string(5) "wp'db"
}
bool(true)
bool(true)
