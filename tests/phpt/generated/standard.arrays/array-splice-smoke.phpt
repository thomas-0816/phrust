--TEST--
Generated standard.arrays: array_splice replacement and reindex smoke
--DESCRIPTION--
module: standard.arrays
generated timestamp: 20260628T000000Z
generator version: phpt-standard-arrays-v2
reason: focused coverage for array_splice() removed values, replacement values, and numeric reindexing from Reference PHP output
--FILE--
<?php
$a = [10, 20, 30, 40];
$removed = array_splice($a, 1, 2, ["x" => "X", 50]);

var_dump($removed);
var_dump($a);
?>
--EXPECT--
array(2) {
  [0]=>
  int(20)
  [1]=>
  int(30)
}
array(4) {
  [0]=>
  int(10)
  [1]=>
  string(1) "X"
  [2]=>
  int(50)
  [3]=>
  int(40)
}
