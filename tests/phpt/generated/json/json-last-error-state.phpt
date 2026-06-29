--TEST--
json: last error state smoke
--DESCRIPTION--
Generated focused coverage for json_last_error() and json_last_error_msg() state transitions.
--FILE--
<?php
var_dump(json_last_error());
var_dump(json_last_error_msg());
var_dump(json_decode('{'));
var_dump(json_last_error());
var_dump(json_last_error_msg());
var_dump(json_decode('[]'));
var_dump(json_last_error());
var_dump(json_last_error_msg());
?>
--EXPECT--
int(0)
string(8) "No error"
NULL
int(4)
string(12) "Syntax error"
array(0) {
}
int(0)
string(8) "No error"
