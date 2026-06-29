--TEST--
sqlite3: prepared statements and status helpers
--DESCRIPTION--
Generated SQLite3 fixture for migration-style prepared statements, positional
and named binding, result fetches, lastInsertRowID, changes, busyTimeout, and
escapeString coverage.
--EXTENSIONS--
sqlite3
--FILE--
<?php
$db = new SQLite3(":memory:");
var_dump($db->busyTimeout(25));
var_dump($db->exec("CREATE TABLE demo (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, qty INTEGER)"));

$qty = 3;
$stmt = $db->prepare("INSERT INTO demo (name, qty) VALUES (?, ?)");
var_dump($stmt instanceof SQLite3Stmt);
var_dump($stmt->bindValue(1, "alpha", SQLITE3_TEXT));
var_dump($stmt->bindParam(2, $qty, SQLITE3_INTEGER));
var_dump($stmt->execute());
var_dump($db->lastInsertRowID());
var_dump($db->changes());

$stmt = $db->prepare("INSERT INTO demo (name, qty) VALUES (:name, :qty)");
var_dump($stmt->bindValue(":name", "beta", SQLITE3_TEXT));
var_dump($stmt->bindValue(":qty", 5, SQLITE3_INTEGER));
var_dump($stmt->execute());

$select = $db->prepare("SELECT id, name, qty FROM demo WHERE qty >= ? ORDER BY id");
var_dump($select->bindValue(1, 3, SQLITE3_INTEGER));
$result = $select->execute();
var_dump($result instanceof SQLite3Result);
var_dump($result->fetchArray(SQLITE3_ASSOC));
var_dump($result->fetchArray(SQLITE3_NUM));
var_dump($result->fetchArray(SQLITE3_ASSOC));
var_dump($db->querySingle("SELECT COUNT(*) FROM demo"));
var_dump($db->escapeString("can't"));
var_dump($db->close());
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
int(1)
int(1)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
array(3) {
  ["id"]=>
  int(1)
  ["name"]=>
  string(5) "alpha"
  ["qty"]=>
  int(3)
}
array(3) {
  [0]=>
  int(2)
  [1]=>
  string(4) "beta"
  [2]=>
  int(5)
}
bool(false)
int(2)
string(6) "can''t"
bool(true)
