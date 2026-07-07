--TEST--
Generated scalar.methods: string object methods
--DESCRIPTION--
module: scalar.methods
generated timestamp: 20260707T120000Z
generator version: phpt-scalar.methods-v1
reason: Lock in the string scalar object methods (trim, upper, lower, length, contains, startsWith, endsWith)
--FILE--
<?php
echo "  Hello World  "->trim() . "\n";
echo "hello"->upper() . "\n";
echo "HELLO"->lower() . "\n";
echo "hello"->length() . "\n";
echo ("hello"->contains("ell") ? "true" : "false") . "\n";
echo ("hello"->contains("xyz") ? "true" : "false") . "\n";
echo ("hello"->startsWith("he") ? "true" : "false") . "\n";
echo ("hello"->startsWith("xyz") ? "true" : "false") . "\n";
echo ("hello"->endsWith("lo") ? "true" : "false") . "\n";
echo ("hello"->endsWith("xyz") ? "true" : "false") . "\n";
echo "..trim.."->trim(".") . "\n";
echo "  hello  "->trim()->upper() . "\n";
?>
--EXPECT--
Hello World
HELLO
hello
5
true
false
true
false
true
false
trim
HELLO
