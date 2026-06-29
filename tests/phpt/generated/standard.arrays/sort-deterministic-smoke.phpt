--TEST--
Generated standard.arrays: deterministic sort, asort, and ksort smoke
--DESCRIPTION--
module: standard.arrays
generated timestamp: 20260628T000000Z
generator version: phpt-standard-arrays-v2
reason: focused coverage for deterministic non-callback sorting builtins mutating arrays by reference from Reference PHP output
--FILE--
<?php
$a = ["b" => 3, "a" => 1, "c" => 2];
var_dump(sort($a));
var_export($a);
echo "\n";

$b = ["b" => 3, "a" => 1, "c" => 2];
var_dump(asort($b));
var_export($b);
echo "\n";

$c = ["b" => 3, "a" => 1, "c" => 2];
var_dump(ksort($c));
var_export($c);
echo "\n";
?>
--EXPECT--
bool(true)
array (
  0 => 1,
  1 => 2,
  2 => 3,
)
bool(true)
array (
  'a' => 1,
  'c' => 2,
  'b' => 3,
)
bool(true)
array (
  'a' => 1,
  'b' => 3,
  'c' => 2,
)
