--TEST--
standard.arrays: array_values smoke
--DESCRIPTION--
Generated focused coverage for array_values() reindexing.
--FILE--
<?php
var_dump(array_values(["a" => 1, 2 => "two", "b" => null]));
?>
--EXPECT--
array(3) {
  [0]=>
  int(1)
  [1]=>
  string(3) "two"
  [2]=>
  NULL
}
