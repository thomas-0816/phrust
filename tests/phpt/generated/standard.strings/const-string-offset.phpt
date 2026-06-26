--TEST--
Generated standard.strings: string offset in a constant expression
--DESCRIPTION--
module: standard.strings
generated timestamp: 20260626T000000Z
generator version: phpt-standard-strings-v1
reason: a string-literal offset like "BAR"[0] is a valid constant expression and folds to the single-byte string, including negative indices (tests/strings/offsets_general.phpt)
--FILE--
<?php
const FOO = "BAR"[0];
const BAZ = "hello"[-1];
var_dump(FOO, BAZ);
?>
--EXPECT--
string(1) "B"
string(1) "o"
