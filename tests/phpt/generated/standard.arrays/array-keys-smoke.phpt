--TEST--
standard.arrays: array_keys smoke
--DESCRIPTION--
Generated focused coverage for array_keys() key extraction and search filtering.
--FILE--
<?php
$input = ["a" => 1, "b" => 2, 3 => "three", -1 => "minus", 4];

var_dump(array_keys($input));
var_dump(array_keys($input, 2));
var_dump(array_keys(["1" => 1, "one" => "1"], "1", true));
?>
--EXPECT--
array(5) {
  [0]=>
  string(1) "a"
  [1]=>
  string(1) "b"
  [2]=>
  int(3)
  [3]=>
  int(-1)
  [4]=>
  int(4)
}
array(1) {
  [0]=>
  string(1) "b"
}
array(1) {
  [0]=>
  string(3) "one"
}
