--TEST--
Generated standard.strings: integer string offset write
--DESCRIPTION--
module: standard.strings
generated timestamp: 20260626T000000Z
generator version: phpt-standard-strings-v1
reason: $s[$i] = $c replaces the byte at $i with the first byte of $c, and writing past the end pads the string with spaces (tests/strings/bug22592.phpt)
--FILE--
<?php
$s = "abcdef";
$s[1] = '*';
$s[3] = '*';
$s[5] = '*';
var_dump($s);
$t = "ab";
$t[5] = 'Z';
var_dump($t);
?>
--EXPECT--
string(6) "a*c*e*"
string(6) "ab   Z"
