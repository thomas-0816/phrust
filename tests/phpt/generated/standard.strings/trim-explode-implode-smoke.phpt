--TEST--
standard.strings: trim explode implode smoke
--DESCRIPTION--
Generated focused coverage for trim(), explode(), and implode().
--FILE--
<?php
var_dump(trim("\0\t phrust \n\0"));
var_dump(trim("xxhelloxy", "xy"));
var_dump(explode("|", "a|b|c", 2));
var_dump(explode("|", "a|b|c", -1));
var_dump(implode(",", ["a", "b", "c"]));
?>
--EXPECT--
string(6) "phrust"
string(5) "hello"
array(2) {
  [0]=>
  string(1) "a"
  [1]=>
  string(3) "b|c"
}
array(2) {
  [0]=>
  string(1) "a"
  [1]=>
  string(1) "b"
}
string(5) "a,b,c"
