--TEST--
pcre: preg_last_error state persists across VM builtin calls
--DESCRIPTION--
Generated focused coverage for invalid-pattern state, canonical PHP error message text, and success reset.
--FILE--
<?php
var_dump(preg_last_error());
var_dump(preg_last_error_msg());
var_dump(@preg_match('/[/', 'x'));
var_dump(preg_last_error());
var_dump(preg_last_error_msg());
var_dump(preg_match('/x/', 'x'));
var_dump(preg_last_error());
var_dump(preg_last_error_msg());
?>
--EXPECT--
int(0)
string(8) "No error"
bool(false)
int(1)
string(14) "Internal error"
int(1)
int(0)
string(8) "No error"
