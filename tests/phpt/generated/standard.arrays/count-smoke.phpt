--TEST--
standard.arrays: count smoke
--DESCRIPTION--
Generated focused coverage for count() and sizeof().
--FILE--
<?php
$nested = ["a" => 1, "b" => [2, 3], "c" => []];

var_dump(count([]));
var_dump(count($nested));
var_dump(count($nested, COUNT_RECURSIVE));
var_dump(sizeof([1, 2, 3]));
?>
--EXPECT--
int(0)
int(3)
int(5)
int(3)
