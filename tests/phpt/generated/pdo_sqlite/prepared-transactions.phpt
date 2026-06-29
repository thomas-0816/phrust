--TEST--
pdo_sqlite: prepared parameters and transaction MVP
--DESCRIPTION--
Generated coverage for PDO_SQLite positional and named parameters,
bound values, object fetch mode, lastInsertId, rowCount, and transaction
commit/rollback behavior.
--EXTENSIONS--
pdo
pdo_sqlite
--FILE--
<?php
var_dump(extension_loaded("pdo_sqlite"));
var_dump(extension_loaded("pdo"));

$db = new PDO("sqlite::memory:");
var_dump($db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_EXCEPTION));
var_dump($db->exec("CREATE TABLE demo (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, qty INTEGER)"));

$stmt = $db->prepare("INSERT INTO demo (name, qty) VALUES (?, ?)");
var_dump($stmt->bindValue(1, "alpha", PDO::PARAM_STR));
var_dump($stmt->bindValue(2, 3, PDO::PARAM_INT));
var_dump($stmt->execute());
var_dump($stmt->rowCount());
var_dump($db->lastInsertId());

$name = "beta";
$stmt = $db->prepare("INSERT INTO demo (name, qty) VALUES (:name, :qty)");
var_dump($stmt->bindParam(":name", $name, PDO::PARAM_STR));
var_dump($stmt->bindValue(":qty", 5, PDO::PARAM_INT));
var_dump($stmt->execute());
var_dump($stmt->rowCount());

$select = $db->prepare("SELECT qty FROM demo WHERE name = :name");
var_dump($select->execute([":name" => "alpha"]));
var_dump($select->fetch(PDO::FETCH_ASSOC));

$select = $db->prepare("SELECT name FROM demo WHERE qty = ?");
var_dump($select->execute([5]));
$row = $select->fetch(PDO::FETCH_OBJ);
var_dump($row instanceof stdClass);
var_dump($row->name);

var_dump($db->beginTransaction());
var_dump($db->exec("INSERT INTO demo (name, qty) VALUES ('rollback', 99)"));
var_dump($db->rollBack());
var_dump($db->query("SELECT COUNT(*) FROM demo WHERE name = 'rollback'")->fetchColumn());

var_dump($db->beginTransaction());
var_dump($db->exec("INSERT INTO demo (name, qty) VALUES ('commit', 7)"));
var_dump($db->commit());
var_dump($db->query("SELECT COUNT(*) FROM demo WHERE name = 'commit'")->fetchColumn());
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
int(0)
bool(true)
bool(true)
bool(true)
int(1)
string(1) "1"
bool(true)
bool(true)
bool(true)
int(1)
bool(true)
array(1) {
  ["qty"]=>
  int(3)
}
bool(true)
bool(true)
string(4) "beta"
bool(true)
int(1)
bool(true)
int(0)
bool(true)
int(1)
bool(true)
int(1)
