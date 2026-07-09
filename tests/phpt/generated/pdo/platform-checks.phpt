--TEST--
pdo: core surface with enabled PDO drivers
--DESCRIPTION--
Generated coverage for PDO extension visibility, core constants,
driver listing, metadata, and error state.
--EXTENSIONS--
pdo
--FILE--
<?php
var_dump(extension_loaded("pdo"));
var_dump(function_exists("pdo_drivers"));
var_dump(pdo_drivers());
var_dump(class_exists("PDO", false));
var_dump(class_exists("PDOException", false));
var_dump(class_exists("PDOStatement", false));
var_dump(class_exists("PDORow", false));
var_dump(PDO::FETCH_ASSOC, PDO::FETCH_NUM, PDO::FETCH_BOTH, PDO::FETCH_COLUMN);
var_dump(PDO::ERRMODE_SILENT, PDO::ERRMODE_WARNING, PDO::ERRMODE_EXCEPTION);
var_dump(PDO::ERR_NONE);

$db = new PDO("sqlite::memory:");
var_dump($db instanceof PDO);
var_dump($db->getAttribute(PDO::ATTR_DRIVER_NAME));
var_dump($db->getAttribute(PDO::ATTR_DEFAULT_FETCH_MODE));
var_dump($db->errorCode());
var_dump($db->errorInfo());
var_dump($db->quote("a'b"));
?>
--EXPECT--
bool(true)
bool(true)
array(3) {
  [0]=>
  string(5) "mysql"
  [1]=>
  string(5) "pgsql"
  [2]=>
  string(6) "sqlite"
}
bool(true)
bool(true)
bool(true)
bool(true)
int(2)
int(3)
int(4)
int(7)
int(0)
int(1)
int(2)
string(5) "00000"
bool(true)
string(6) "sqlite"
int(4)
string(5) "00000"
array(3) {
  [0]=>
  string(5) "00000"
  [1]=>
  NULL
  [2]=>
  NULL
}
string(6) "'a''b'"
