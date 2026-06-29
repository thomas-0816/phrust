--TEST--
Generated standard.arrays: array_key_exists and key_exists smoke
--DESCRIPTION--
module: standard.arrays
generated timestamp: 20260628T000000Z
generator version: phpt-standard-arrays-v2
reason: focused coverage for array_key_exists() and key_exists() with null values, integer keys, string keys, and missing keys from Reference PHP output
--FILE--
<?php
$a = ["x" => null, 0 => "zero", "08" => "zero-eight"];

var_dump(array_key_exists("x", $a));
var_dump(isset($a["x"]));
var_dump(array_key_exists(0, $a));
var_dump(array_key_exists("08", $a));
var_dump(key_exists("missing", $a));
?>
--EXPECT--
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)
