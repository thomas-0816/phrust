--TEST--
mbstring: bounded UTF-8 position functions
--DESCRIPTION--
Focused mbstring UTF-8 coverage for mb_strpos and mb_stripos.
--SKIPIF--
<?php
if (!extension_loaded("mbstring")) {
    die("skip mbstring extension not loaded\n");
}
?>
--FILE--
<?php
var_dump(mb_strpos("Aé日é", "é", 0, "UTF-8"));
var_dump(mb_strpos("Aé日é", "é", 2, "UTF-8"));
var_dump(mb_strpos("Aé日é", "é", -2, "UTF-8"));
var_dump(mb_strpos("abc", "z", 0, "UTF-8"));
var_dump(mb_strpos("abc", "", 0, "UTF-8"));
var_dump(mb_stripos("Aé日É", "é", 0, "UTF-8"));
var_dump(mb_stripos("ÄÖÜ abc", "ö", 0, "UTF-8"));
var_dump(mb_stripos("Straße", "SS", 0, "UTF-8"));
?>
--EXPECT--
int(1)
int(3)
int(3)
bool(false)
int(0)
int(1)
int(1)
bool(false)
