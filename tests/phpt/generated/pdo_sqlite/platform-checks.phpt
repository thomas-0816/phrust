--TEST--
pdo_sqlite: SQLite DSN query and statement MVP
--DESCRIPTION--
Generated coverage for PDO_SQLite query, prepare/execute, fetch
modes, and local file persistence.
--EXTENSIONS--
pdo
pdo_sqlite
--FILE--
<?php
var_dump(extension_loaded("pdo_sqlite"));
var_dump(extension_loaded("pdo"));

$db = new PDO("sqlite::memory:");
var_dump($db->exec("CREATE TABLE demo (id INTEGER, name TEXT)"));
var_dump($db->exec("INSERT INTO demo VALUES (1, 'alpha'), (2, 'beta')"));

$stmt = $db->query("SELECT id, name FROM demo ORDER BY id");
var_dump($stmt instanceof PDOStatement);
var_dump($stmt->queryString);
var_dump($stmt->columnCount());
var_dump($stmt->fetch(PDO::FETCH_ASSOC));
var_dump($stmt->fetch(PDO::FETCH_NUM));
var_dump($stmt->fetch());
var_dump($stmt->closeCursor());

$prepared = $db->prepare("SELECT name FROM demo WHERE id = 2");
var_dump($prepared instanceof PDOStatement);
var_dump($prepared->execute());
var_dump($prepared->fetchColumn());

$all = $db->query("SELECT id, name FROM demo ORDER BY id");
var_dump($all->fetchAll(PDO::FETCH_ASSOC));

$path = __DIR__ . "/pdo-sqlite-mvp.db";
@unlink($path);
$file = new PDO("sqlite:" . $path);
var_dump($file->exec("CREATE TABLE persisted (v TEXT)"));
var_dump($file->exec("INSERT INTO persisted VALUES ('stored')"));
$file = null;
$file = new PDO("sqlite:" . $path);
var_dump($file->query("SELECT v FROM persisted")->fetchColumn());
var_dump(unlink($path));
?>
--EXPECT--
bool(true)
bool(true)
int(0)
int(2)
bool(true)
string(37) "SELECT id, name FROM demo ORDER BY id"
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
bool(true)
bool(true)
bool(true)
string(4) "beta"
array(2) {
  [0]=>
  array(2) {
    ["id"]=>
    int(1)
    ["name"]=>
    string(5) "alpha"
  }
  [1]=>
  array(2) {
    ["id"]=>
    int(2)
    ["name"]=>
    string(4) "beta"
  }
}
int(0)
int(1)
string(6) "stored"
bool(true)
