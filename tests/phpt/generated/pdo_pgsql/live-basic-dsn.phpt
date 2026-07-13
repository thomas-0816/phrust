--TEST--
pdo_pgsql: live query, prepared statements, fetch modes, and transactions through DSN gate
--DESCRIPTION--
Generated opt-in live PostgreSQL PDO query, errmode, fetch, and transaction contract.
--SKIPIF--
<?php
if (!extension_loaded("pdo") || !extension_loaded("pdo_pgsql")) {
    die("skip pdo_pgsql extension is not loaded");
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

$dsn = "pgsql:host=" . $host;
if ($port !== null) {
    $dsn .= ";port=" . $port;
}
if ($dbName !== "") {
    $dsn .= ";dbname=" . $dbName;
}
$dsn .= ";sslmode=disable";

$pdo = new PDO($dsn, $user, $pass);
var_dump($pdo->getAttribute(PDO::ATTR_DRIVER_NAME));
var_dump($pdo->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_EXCEPTION));

try {
    $pdo->query("SELECT phrust_missing_column FROM phrust_missing_table");
} catch (PDOException $e) {
    echo "errmode:", (strlen($e->getMessage()) > 0 ? "message" : "empty"), "\n";
}

$result = $pdo->query("SELECT 1 AS one");
var_dump($result instanceof PDOStatement);
var_dump($result->fetch(PDO::FETCH_ASSOC));

$statement = $pdo->prepare("SELECT ?::bigint AS two");
var_dump($statement->execute([2]));
var_dump($statement->fetchColumn());
var_dump($pdo->quote("a'b"));

$pdo->exec("DROP TABLE IF EXISTS phrust_pdo_pgsql_demo");
var_dump($pdo->exec("CREATE TEMP TABLE phrust_pdo_pgsql_demo (id INTEGER, name TEXT)"));

var_dump($pdo->inTransaction());
var_dump($pdo->beginTransaction());
var_dump($pdo->inTransaction());
$insert = $pdo->prepare("INSERT INTO phrust_pdo_pgsql_demo (id, name) VALUES (?, ?)");
var_dump($insert->execute([1, "rollback"]));
var_dump($pdo->rollBack());
var_dump($pdo->inTransaction());
var_dump($pdo->query("SELECT COUNT(*) AS count FROM phrust_pdo_pgsql_demo WHERE name = 'rollback'")->fetchColumn());

var_dump($pdo->beginTransaction());
var_dump($pdo->exec("INSERT INTO phrust_pdo_pgsql_demo (id, name) VALUES (2, 'commit')"));
var_dump($pdo->commit());
var_dump($pdo->inTransaction());

$assoc = $pdo->query("SELECT id, name FROM phrust_pdo_pgsql_demo ORDER BY id");
var_dump($assoc->fetch(PDO::FETCH_ASSOC));

$num = $pdo->query("SELECT id, name FROM phrust_pdo_pgsql_demo ORDER BY id");
var_dump($num->fetch(PDO::FETCH_NUM));

$both = $pdo->query("SELECT id, name FROM phrust_pdo_pgsql_demo ORDER BY id");
var_dump($both->fetch(PDO::FETCH_BOTH));

$object = $pdo->query("SELECT id, name FROM phrust_pdo_pgsql_demo ORDER BY id");
$row = $object->fetch(PDO::FETCH_OBJ);
var_dump($row instanceof stdClass);
var_dump($row->name);

$column = $pdo->query("SELECT name FROM phrust_pdo_pgsql_demo ORDER BY id");
var_dump($column->fetch(PDO::FETCH_COLUMN));
?>
--EXPECT--
string(5) "pgsql"
bool(true)
errmode:message
bool(true)
array(1) {
  ["one"]=>
  int(1)
}
bool(true)
int(2)
string(6) "'a''b'"
int(0)
bool(false)
bool(true)
bool(true)
bool(true)
bool(true)
bool(false)
int(0)
bool(true)
int(1)
bool(true)
bool(false)
array(2) {
  ["id"]=>
  int(2)
  ["name"]=>
  string(6) "commit"
}
array(2) {
  [0]=>
  int(2)
  [1]=>
  string(6) "commit"
}
array(4) {
  [0]=>
  int(2)
  [1]=>
  string(6) "commit"
  ["id"]=>
  int(2)
  ["name"]=>
  string(6) "commit"
}
bool(true)
string(6) "commit"
string(6) "commit"
