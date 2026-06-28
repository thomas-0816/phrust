--TEST--
wp.db-network: mysqli object API default-off wpdb shape
--DESCRIPTION--
Prompt 3.4 contract: the mysqli object surface exists for WordPress-style
initialization, while real network connection remains DSN-gated.
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
$db = mysqli_init();
var_dump($db instanceof mysqli);
var_dump($db->real_connect("ignored-host", "ignored-user", "ignored-password", "ignored-db"));
var_dump($db->connect_errno > 0);
var_dump($db->connect_error !== "");
?>
--EXPECT--
bool(true)
bool(false)
bool(true)
bool(true)
