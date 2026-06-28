--TEST--
Generated zend.functions: variadic builtin accepts extra args
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260627T000000Z
generator version: phpt-zend-functions-v1
reason: arginfo-backed variadic builtin arity handling
--FILE--
<?php
var_dump("a", "b", "c");
?>
--EXPECT--
string(1) "a"
string(1) "b"
string(1) "c"
