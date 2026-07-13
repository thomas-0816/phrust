--TEST--
mysqli: mysqlnd operations over SQLite compatibility adapter
--DESCRIPTION--
Generated phrust-only fixture for mysqli/mysqlnd-style options, stats,
transaction, multi-result, and prepared metadata behavior. The SQLite adapter
is deterministic application coverage only and is not MySQL wire-protocol or
mysqlnd parity.
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
$db = mysqli_init();
var_dump(mysqli_options($db, MYSQLI_OPT_CONNECT_TIMEOUT, 1));
var_dump(mysqli_real_connect($db, "compat-host", "user", "pass", "app", 3306, "/tmp/mysql.sock"));
var_dump(mysqli_ping($db));

$clientStats = mysqli_get_client_stats();
$connectionStats = mysqli_get_connection_stats($db);
var_dump(is_array($clientStats), array_key_exists("active_connections", $clientStats));
var_dump(is_array($connectionStats), array_key_exists("bytes_sent", $connectionStats));

var_dump(mysqli_autocommit($db, false));
var_dump(mysqli_begin_transaction($db));
var_dump(mysqli_query($db, "CREATE TABLE items (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)"));
var_dump(mysqli_query($db, "INSERT INTO items (name) VALUES ('tx')"));
var_dump(mysqli_commit($db));

var_dump(mysqli_begin_transaction($db));
var_dump(mysqli_query($db, "INSERT INTO items (name) VALUES ('rollback')"));
var_dump(mysqli_rollback($db));
$count = mysqli_query($db, "SELECT COUNT(*) AS c FROM items");
var_dump(mysqli_fetch_assoc($count));

var_dump(mysqli_multi_query($db, "SELECT 1 AS one; SELECT 2 AS two"));
$first = mysqli_store_result($db);
var_dump($first instanceof mysqli_result);
var_dump(mysqli_fetch_assoc($first));
var_dump(mysqli_more_results($db));
var_dump(mysqli_next_result($db));
$second = mysqli_use_result($db);
var_dump($second instanceof mysqli_result);
var_dump(mysqli_fetch_row($second));
var_dump(mysqli_more_results($db));

$id = 1;
$stmt = mysqli_prepare($db, "SELECT id, name FROM items WHERE id = ?");
var_dump(mysqli_stmt_bind_param($stmt, "i", $id));
var_dump(mysqli_stmt_execute($stmt));
$metadata = mysqli_stmt_result_metadata($stmt);
var_dump($metadata instanceof mysqli_result);
$fields = mysqli_fetch_fields($metadata);
var_dump(count($fields), $fields[0]->name, $fields[1]->name);
$outId = null;
$outName = null;
var_dump(mysqli_stmt_bind_result($stmt, $outId, $outName));
var_dump(mysqli_stmt_fetch($stmt));
var_dump($outId, $outName);
$result = mysqli_stmt_get_result($stmt);
var_dump($result instanceof mysqli_result);
var_dump(mysqli_fetch_assoc($result));
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
array(1) {
  ["c"]=>
  int(1)
}
bool(true)
bool(true)
array(1) {
  ["one"]=>
  int(1)
}
bool(true)
bool(true)
bool(true)
array(1) {
  [0]=>
  int(2)
}
bool(false)
bool(true)
bool(true)
bool(true)
int(2)
string(2) "id"
string(4) "name"
bool(true)
bool(true)
int(1)
string(2) "tx"
bool(true)
bool(false)
