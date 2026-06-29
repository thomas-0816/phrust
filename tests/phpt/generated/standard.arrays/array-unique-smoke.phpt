--TEST--
Generated standard.arrays: array_unique comparison modes smoke
--DESCRIPTION--
module: standard.arrays
generated timestamp: 20260628T000000Z
generator version: phpt-standard-arrays-v2
reason: focused coverage for array_unique() key preservation and default versus SORT_REGULAR comparison behavior from Reference PHP output
--FILE--
<?php
$values = ["a" => "1", "b" => 1, "c" => "01", "d" => "1"];

var_dump(array_unique($values));
var_dump(array_unique($values, SORT_REGULAR));
?>
--EXPECT--
array(2) {
  ["a"]=>
  string(1) "1"
  ["c"]=>
  string(2) "01"
}
array(1) {
  ["a"]=>
  string(1) "1"
}
