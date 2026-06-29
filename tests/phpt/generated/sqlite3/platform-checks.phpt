--TEST--
sqlite3: local MVP query and result behavior
--DESCRIPTION--
Generated coverage for deterministic SQLite3 :memory: and local file
database behavior.
--EXTENSIONS--
sqlite3
--FILE--
<?php
var_dump(extension_loaded("sqlite3"));
var_dump(class_exists("SQLite3", false));
var_dump(class_exists("SQLite3Stmt", false));
var_dump(class_exists("SQLite3Result", false));
var_dump(class_exists("SQLite3Exception", false));
var_dump(SQLITE3_ASSOC, SQLITE3_NUM, SQLITE3_BOTH);
var_dump(SQLITE3_INTEGER, SQLITE3_FLOAT, SQLITE3_TEXT, SQLITE3_BLOB, SQLITE3_NULL);
var_dump(SQLITE3_OPEN_READONLY, SQLITE3_OPEN_READWRITE, SQLITE3_OPEN_CREATE, SQLITE3_DETERMINISTIC);

$db = new SQLite3(":memory:");
var_dump($db instanceof SQLite3);
var_dump($db->exec("CREATE TABLE demo (id INTEGER, name TEXT)"));
var_dump($db->exec("INSERT INTO demo VALUES (1, 'alpha'), (2, 'beta')"));
var_dump($db->querySingle("SELECT name FROM demo WHERE id = 1"));
var_dump($db->querySingle("SELECT id, name FROM demo WHERE id = 2", true));

$result = $db->query("SELECT id, name FROM demo ORDER BY id");
var_dump($result instanceof SQLite3Result);
var_dump($result->numColumns());
var_dump($result->fetchArray(SQLITE3_ASSOC));
var_dump($result->fetchArray(SQLITE3_NUM));
var_dump($result->fetchArray());
var_dump($result->reset());
var_dump($result->fetchAll(SQLITE3_ASSOC));
var_dump($result->finalize());
var_dump($db->lastErrorCode());
var_dump($db->lastErrorMsg());
var_dump($db->close());

$path = __DIR__ . "/sqlite3-mvp.db";
@unlink($path);
$file = new SQLite3($path);
var_dump($file->exec("CREATE TABLE persisted (v TEXT)"));
var_dump($file->exec("INSERT INTO persisted VALUES ('stored')"));
var_dump($file->close());
$file = new SQLite3($path);
var_dump($file->querySingle("SELECT v FROM persisted"));
var_dump($file->close());
var_dump(unlink($path));
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
int(1)
int(2)
int(3)
int(1)
int(2)
int(3)
int(4)
int(5)
int(1)
int(2)
int(4)
int(2048)
bool(true)
bool(true)
bool(true)
string(5) "alpha"
array(2) {
  ["id"]=>
  int(2)
  ["name"]=>
  string(4) "beta"
}
bool(true)
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
bool(true)
int(0)
string(12) "not an error"
bool(true)
bool(true)
bool(true)
bool(true)
string(6) "stored"
bool(true)
bool(true)
