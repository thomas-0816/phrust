--TEST--
Generated standard.arrays: range numeric and character smoke
--DESCRIPTION--
module: standard.arrays
generated timestamp: 20260628T000000Z
generator version: phpt-standard-arrays-v2
reason: focused coverage for range() increasing numeric, character step, and decreasing default-step output from Reference PHP output
--FILE--
<?php
var_dump(range(1, 5, 2));
var_dump(range("a", "d", 2));
var_dump(range(3, 1));
?>
--EXPECT--
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(3)
  [2]=>
  int(5)
}
array(2) {
  [0]=>
  string(1) "a"
  [1]=>
  string(1) "c"
}
array(3) {
  [0]=>
  int(3)
  [1]=>
  int(2)
  [2]=>
  int(1)
}
