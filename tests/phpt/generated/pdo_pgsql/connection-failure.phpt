--TEST--
pdo_pgsql: constructor failures are catchable PDOException objects
--DESCRIPTION--
Generated coverage for PostgreSQL PDO connection failure handling without
requiring a live PostgreSQL server.
--EXTENSIONS--
pdo
pdo_pgsql
--FILE--
<?php
try {
    $pdo = new PDO("sqlite::memory:");
    $pdo->__construct("pgsql:host=127.0.0.1;port=abc");
    echo "not-thrown\n";
} catch (PDOException $e) {
    echo "caught:", ($e instanceof Exception ? "exception" : "no"), ":",
        (strlen($e->getMessage()) > 0 ? "message" : "empty"), "\n";
}
?>
--EXPECT--
caught:exception:message
