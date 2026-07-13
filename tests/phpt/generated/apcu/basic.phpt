--TEST--
APCu process-local cache basics
--SKIPIF--
<?php if (!extension_loaded("apcu")) die("skip apcu extension not loaded"); ?>
--FILE--
<?php
var_dump(apcu_enabled());
var_dump(apcu_store("k", "v"));
var_dump(apcu_fetch("k", $ok));
var_dump($ok);
var_dump(apcu_add("k", "other"));
var_dump(apcu_exists("k"));
var_dump(apcu_delete("k"));
var_dump(apcu_fetch("k"));
var_dump(apcu_store("count", 4));
var_dump(apcu_inc("count", 3, $inc_ok));
var_dump($inc_ok);
var_dump(apcu_dec("count", 2, $dec_ok));
var_dump($dec_ok);
$info = apcu_cache_info();
var_dump(is_array($info));
var_dump($info["num_entries"]);
var_dump(array_key_exists("cache_list", $info));
$sma = apcu_sma_info();
var_dump(is_array($sma));
var_dump($sma["num_seg"]);
function apcu_entry_value($key) {
    echo "entry-generator:$key\n";
    return strtoupper($key);
}
var_dump(apcu_entry("entry-key", "apcu_entry_value"));
var_dump(apcu_entry("entry-key", function ($key) { return "miss"; }));
?>
--EXPECT--
bool(true)
bool(true)
string(1) "v"
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)
bool(true)
int(7)
bool(true)
int(5)
bool(true)
bool(true)
int(1)
bool(true)
bool(true)
int(1)
entry-generator:entry-key
string(9) "ENTRY-KEY"
string(9) "ENTRY-KEY"
