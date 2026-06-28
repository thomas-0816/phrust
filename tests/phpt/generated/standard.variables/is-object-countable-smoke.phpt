--TEST--
standard.variables: is_object and is_countable smoke
--DESCRIPTION--
Generated focused coverage for object and countable type helpers.
--FILE--
<?php
$object = new stdClass();
$array = [1, 2, 3];
$string = "php";
$null = null;

var_dump(is_object($object));
var_dump(is_object($array));
var_dump(is_countable($array));
var_dump(is_countable($object));
var_dump(is_countable($string));
var_dump(is_countable($null));
?>
--EXPECT--
bool(true)
bool(false)
bool(true)
bool(false)
bool(false)
bool(false)
