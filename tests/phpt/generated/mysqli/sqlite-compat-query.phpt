--TEST--
mysqli: SQLite compatibility adapter query and status basics
--DESCRIPTION--
Generated mysqli compatibility fixture for deterministic application query,
fetch, insert id, affected rows, and error-state coverage. This fixture is a
phrust-only compatibility contract and does not claim MySQL protocol parity.
--SKIPIF--
<?php
if (!extension_loaded("mysqli")) {
    die("skip mysqli extension is not loaded");
}
if (basename(PHP_BINARY) !== "phrust-php") {
    die("skip phrust-only mysqli SQLite compatibility fixture");
}
?>
--ENV--
PHRUST_MYSQLI_SQLITE_COMPAT=1
--FILE--
<?php
$db = mysqli_connect("compat-host", "user", "pass", "app");
var_dump($db instanceof mysqli);
var_dump(mysqli_query($db, "CREATE TABLE items (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)"));
var_dump(mysqli_query($db, "INSERT INTO items (name) VALUES ('alpha')"));
var_dump(mysqli_insert_id($db));
var_dump(mysqli_affected_rows($db));
var_dump($db->insert_id);
var_dump($db->affected_rows);
var_dump(mysqli_query($db, "INSERT INTO items (name) VALUES ('beta')"));
$result = mysqli_query($db, "SELECT id, name FROM items ORDER BY id");
var_dump($result instanceof mysqli_result);
var_dump(mysqli_num_rows($result));
var_dump(mysqli_num_fields($result));
var_dump(mysqli_fetch_assoc($result));
var_dump(mysqli_fetch_array($result, MYSQLI_NUM));
var_dump(mysqli_fetch_assoc($result));
var_dump(mysqli_query($db, "SELECT missing FROM items"));
var_dump(mysqli_errno($db) > 0);
var_dump(mysqli_error($db) !== "");
var_dump(mysqli_close($db));
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
int(1)
int(1)
int(1)
int(1)
bool(true)
bool(true)
int(2)
int(2)
array(2) {
  ["id"]=>
  int(1)
  ["name"]=>
  string(5) "alpha"
}
array(2) {
  [0]=>
  int(2)
  [1]=>
  string(4) "beta"
}
bool(false)
bool(false)
bool(true)
bool(true)
bool(true)
