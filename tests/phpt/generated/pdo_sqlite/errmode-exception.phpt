--TEST--
pdo_sqlite: ERRMODE_EXCEPTION maps SQLite failures to PDOException
--DESCRIPTION--
Generated coverage for selected PDO SQLite exception-mode behavior:
statement/query failures still report connection error state in silent mode,
and `PDO::ERRMODE_EXCEPTION` raises catchable `PDOException` objects.
--EXTENSIONS--
pdo
pdo_sqlite
--FILE--
<?php
$db = new PDO("sqlite::memory:");
var_dump($db->exec("SELECT missing"));
var_dump($db->errorCode());
$info = $db->errorInfo();
var_dump($info[0]);

var_dump($db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_EXCEPTION));
try {
    $db->query("SELECT missing");
} catch (PDOException $e) {
    echo "caught:", ($e instanceof Exception ? "exception" : "no"), ":",
        (strlen($e->getMessage()) > 0 ? "message" : "empty"), "\n";
}

$stmt = $db->prepare("INSERT INTO missing VALUES (?)");
try {
    $stmt->execute([1]);
} catch (PDOException $e) {
    echo "stmt:", (strlen($e->getMessage()) > 0 ? "message" : "empty"), "\n";
}
?>
--EXPECT--
bool(false)
string(5) "HY000"
string(5) "HY000"
bool(true)
caught:exception:message
stmt:message
