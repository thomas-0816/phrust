--TEST--
standard.strings: strpos and str_contains smoke
--DESCRIPTION--
Generated focused coverage for binary-safe strpos() and str_contains().
--FILE--
<?php
var_dump(strpos("a\0b\0c", "b\0"));
var_dump(strpos("abcdef", "z"));
var_dump(str_contains("", ""));
var_dump(str_contains("abcdef", "cd"));
?>
--EXPECT--
int(2)
bool(false)
bool(true)
bool(true)
