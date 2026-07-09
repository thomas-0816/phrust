--TEST--
APCu request-local TTL and clear-cache behavior
--SKIPIF--
<?php if (!extension_loaded("apcu")) die("skip apcu extension not loaded"); ?>
--FILE--
<?php
var_dump(apcu_store("ttl", "value", 1));
var_dump(apcu_exists("ttl"));
sleep(2);
var_dump(apcu_fetch("ttl", $ok));
var_dump($ok);
var_dump(apcu_add("ttl", "after-expiry", 1));
var_dump(apcu_fetch("ttl"));
var_dump(apcu_store("persist", "v"));
var_dump(apcu_clear_cache());
var_dump(apcu_exists("ttl"));
var_dump(apcu_exists("persist"));
$info = apcu_cache_info();
var_dump($info["num_entries"]);
?>
--EXPECT--
bool(true)
bool(true)
bool(false)
bool(false)
bool(true)
string(12) "after-expiry"
bool(true)
bool(true)
bool(false)
bool(false)
int(0)
