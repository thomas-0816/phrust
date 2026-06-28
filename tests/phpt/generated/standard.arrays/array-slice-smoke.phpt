--TEST--
standard.arrays: array_slice smoke
--DESCRIPTION--
Generated focused coverage for array_slice() offsets, null length, and preserved keys.
--FILE--
<?php
$input = ["a" => 1, 5 => "five", 6 => "six", "b" => 2, 7 => "seven"];

var_dump(array_slice($input, 1, 3));
var_dump(array_slice($input, 1, 3, true));
var_dump(array_slice($input, -2, null, true));
?>
--EXPECT--
array(3) {
  [0]=>
  string(4) "five"
  [1]=>
  string(3) "six"
  ["b"]=>
  int(2)
}
array(3) {
  [5]=>
  string(4) "five"
  [6]=>
  string(3) "six"
  ["b"]=>
  int(2)
}
array(2) {
  ["b"]=>
  int(2)
  [7]=>
  string(5) "seven"
}
