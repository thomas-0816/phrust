--TEST--
Generated standard.strings: str_replace and ASCII case conversion smoke
--DESCRIPTION--
module: standard.strings
generated timestamp: 20260628T000000Z
generator version: phpt-standard-strings-v2
reason: focused coverage for str_replace() array subjects and replacement count plus strtolower()/strtoupper() ASCII conversion from Reference PHP output
--FILE--
<?php
$result = str_replace(["red", "blue"], ["green", "yellow"], ["red blue", "red"], $count);

var_dump($result);
var_dump($count);
echo strtolower("MiXeD123!"), "\n";
echo strtoupper("MiXeD123!"), "\n";
?>
--EXPECT--
array(2) {
  [0]=>
  string(12) "green yellow"
  [1]=>
  string(5) "green"
}
int(3)
mixed123!
MIXED123!
