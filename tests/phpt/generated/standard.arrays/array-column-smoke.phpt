--TEST--
Generated standard.arrays: array_column values and index keys smoke
--DESCRIPTION--
module: standard.arrays
generated timestamp: 20260628T000000Z
generator version: phpt-standard-arrays-v2
reason: focused coverage for array_column() column extraction, index keys, null column values, and missing index fallback from Reference PHP output
--FILE--
<?php
$rows = [
    ["id" => 10, "name" => "A", "group" => "x"],
    ["id" => 20, "name" => "B", "group" => "y"],
    ["id" => 30, "name" => "C"],
];

var_dump(array_column($rows, "name", "id"));
var_dump(array_column($rows, null, "group"));
?>
--EXPECT--
array(3) {
  [10]=>
  string(1) "A"
  [20]=>
  string(1) "B"
  [30]=>
  string(1) "C"
}
array(3) {
  ["x"]=>
  array(3) {
    ["id"]=>
    int(10)
    ["name"]=>
    string(1) "A"
    ["group"]=>
    string(1) "x"
  }
  ["y"]=>
  array(3) {
    ["id"]=>
    int(20)
    ["name"]=>
    string(1) "B"
    ["group"]=>
    string(1) "y"
  }
  [0]=>
  array(2) {
    ["id"]=>
    int(30)
    ["name"]=>
    string(1) "C"
  }
}
