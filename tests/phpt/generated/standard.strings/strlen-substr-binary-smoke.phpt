--TEST--
standard.strings: strlen and substr binary smoke
--DESCRIPTION--
Generated focused coverage for binary-safe strlen() and substr() offsets.
--FILE--
<?php
var_dump(strlen("a\0b"));
var_dump(substr("abcdef", 2));
var_dump(substr("abcdef", -3, 2));
var_dump(substr("abcdef", 1, -2));
?>
--EXPECT--
int(3)
string(4) "cdef"
string(2) "de"
string(3) "bcd"
