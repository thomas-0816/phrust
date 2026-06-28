--TEST--
standard.strings: strtok state smoke
--DESCRIPTION--
Generated focused coverage for request-local strtok() continuation state.
--FILE--
<?php
var_dump(strtok("one,two;three", ",;"));
var_dump(strtok(",;"));
var_dump(strtok(",;"));
var_dump(strtok(",;"));
var_dump(strtok("alpha beta", " "));
var_dump(strtok(" "));
?>
--EXPECT--
string(3) "one"
string(3) "two"
string(5) "three"
bool(false)
string(5) "alpha"
string(4) "beta"
