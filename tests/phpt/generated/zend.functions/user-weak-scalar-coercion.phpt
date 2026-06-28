--TEST--
Generated zend.functions: weak scalar coercion for user function parameters
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260627T000000Z
generator version: phpt-zend-functions-v1
reason: weak-mode user function scalar parameter checks coerce compatible scalar values
--FILE--
<?php
function prompt13_accept_int(int $value) {
    return gettype($value) . ":" . $value;
}

function prompt13_accept_string(string $value) {
    return gettype($value) . ":" . $value;
}

echo prompt13_accept_int("42"), "\n";
echo prompt13_accept_string(42), "\n";
?>
--EXPECT--
integer:42
string:42
