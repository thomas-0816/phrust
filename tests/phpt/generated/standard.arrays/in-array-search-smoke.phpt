--TEST--
Generated standard.arrays: in_array and array_search strictness smoke
--DESCRIPTION--
module: standard.arrays
generated timestamp: 20260628T000000Z
generator version: phpt-standard-arrays-v2
reason: focused coverage for in_array() and array_search() loose/strict comparisons and key returns from Reference PHP output
--FILE--
<?php
$values = ["a" => "10", "b" => 10, "c" => "needle"];

var_dump(in_array(10, $values));
var_dump(in_array(10, $values, true));
var_dump(array_search("10", $values, true));
var_dump(array_search("needle", $values));
?>
--EXPECT--
bool(true)
bool(true)
string(1) "a"
string(1) "c"
