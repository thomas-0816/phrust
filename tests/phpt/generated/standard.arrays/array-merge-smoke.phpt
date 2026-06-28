--TEST--
standard.arrays: array_merge smoke
--DESCRIPTION--
Generated focused coverage for array_merge() string-key replacement and numeric-key reindexing.
--FILE--
<?php
var_dump(array_merge(["a" => 1, 4 => "four"], ["a" => 2, 9 => "nine", "b" => 3], []));
var_dump(array_merge());
?>
--EXPECT--
array(4) {
  ["a"]=>
  int(2)
  [0]=>
  string(4) "four"
  [1]=>
  string(4) "nine"
  ["b"]=>
  int(3)
}
array(0) {
}
