--TEST--
wp.db-network: mysqli connect is default-off without DSN
--DESCRIPTION--
Prompt 3.3 contract: mysqli is loaded, but live host connections require the
explicit PHRUST_MYSQL_TEST_DSN gate and never fake success.
--SKIPIF--
<?php
if (!extension_loaded("mysqli")) {
    die("skip mysqli extension is not loaded");
}
if (getenv("PHRUST_MYSQL_TEST_DSN") !== false && getenv("PHRUST_MYSQL_TEST_DSN") !== "") {
    die("skip PHRUST_MYSQL_TEST_DSN is configured");
}
?>
--FILE--
<?php
unset($_ENV["PHRUST_MYSQL_TEST_DSN"]);
$db = mysqli_connect("ignored-host", "ignored-user", "ignored-password", "ignored-db");
var_dump($db);
var_dump(mysqli_connect_errno() > 0);
var_dump(mysqli_connect_error() !== "");
?>
--EXPECT--
bool(false)
bool(true)
bool(true)
