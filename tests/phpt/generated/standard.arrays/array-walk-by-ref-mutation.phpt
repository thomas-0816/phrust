--TEST--
Generated standard.arrays: array_walk callback by-reference mutation
--DESCRIPTION--
module: standard.arrays
generated timestamp: 20260710T000000Z
generator version: phpt-standard-arrays-v3
reason: coverage for array_walk callback dispatch mutating array elements by reference from Reference PHP output
--FILE--
<?php
$values = [1, 2];
var_dump(array_walk($values, function(&$value) { $value = $value * 10; }));
var_export($values);
echo "\n";
?>
--EXPECT--
bool(true)
array (
  0 => 10,
  1 => 20,
)
