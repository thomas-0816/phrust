--TEST--
standard.strings: printf and sprintf smoke
--DESCRIPTION--
Generated focused coverage for printf() output and sprintf() return formatting.
--FILE--
<?php
printf("%s:%04d:%.2f\n", "id", 7, 1.25);
var_dump(sprintf("%s:%d:%X", "hex", 255, 255));
?>
--EXPECT--
id:0007:1.25
string(10) "hex:255:FF"
