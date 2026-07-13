--TEST--
pgsql: live query, prepare, fetch, and escape through DSN gate
--DESCRIPTION--
Generated opt-in live PostgreSQL procedural pgsql contract.
--SKIPIF--
<?php
if (!extension_loaded("pgsql")) {
    die("skip pgsql extension is not loaded");
}
if (getenv("PHRUST_POSTGRES_TEST_DSN") === false || getenv("PHRUST_POSTGRES_TEST_DSN") === "") {
    die("skip PHRUST_POSTGRES_TEST_DSN is not configured");
}
?>
--FILE--
<?php
$parts = parse_url(getenv("PHRUST_POSTGRES_TEST_DSN"));
$host = $parts["host"] ?? "127.0.0.1";
$user = isset($parts["user"]) ? rawurldecode($parts["user"]) : "";
$pass = isset($parts["pass"]) ? rawurldecode($parts["pass"]) : "";
$dbName = isset($parts["path"]) ? ltrim($parts["path"], "/") : "";
$port = $parts["port"] ?? null;

$conninfo = "host=" . $host . " sslmode=disable";
if ($port !== null) {
    $conninfo .= " port=" . $port;
}
if ($dbName !== "") {
    $conninfo .= " dbname=" . $dbName;
}
if ($user !== "") {
    $conninfo .= " user=" . $user;
}
if ($pass !== "") {
    $conninfo .= " password=" . $pass;
}

$conn = pg_connect($conninfo);
var_dump($conn !== false);

$result = pg_query($conn, "SELECT 1::bigint AS one, 'two' AS label");
var_dump($result !== false);
var_dump(pg_num_rows($result));
var_dump(pg_num_fields($result));
var_dump(pg_fetch_assoc($result));

$again = pg_query($conn, "SELECT 3::bigint AS three");
var_dump(pg_fetch_result($again, 0, "three"));

$prepared = pg_prepare($conn, "phrust_pgsql_live", "SELECT $1::bigint AS four");
var_dump($prepared !== false);
$executed = pg_execute($conn, "phrust_pgsql_live", [4]);
var_dump(pg_fetch_result($executed, 0, 0));

$paramed = pg_query_params($conn, 'SELECT $1::bigint AS five, $2::text AS label', [5, "five"]);
var_dump(pg_fetch_assoc($paramed));
var_dump(pg_result_error($paramed));

$pconn = pg_pconnect($conninfo);
var_dump($pconn !== false);
var_dump(pg_close($pconn));

var_dump(pg_escape_string($conn, "a'b"));
var_dump(pg_escape_literal($conn, "a'b"));
var_dump(pg_escape_identifier($conn, 'a"b'));
?>
--EXPECT--
bool(true)
bool(true)
int(1)
int(2)
array(2) {
  ["one"]=>
  int(1)
  ["label"]=>
  string(3) "two"
}
int(3)
bool(true)
int(4)
array(2) {
  ["five"]=>
  int(5)
  ["label"]=>
  string(4) "five"
}
string(0) ""
bool(true)
bool(true)
string(4) "a''b"
string(6) "'a''b'"
string(6) ""a""b""
