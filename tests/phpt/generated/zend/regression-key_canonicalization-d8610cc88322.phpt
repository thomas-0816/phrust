--TEST--
PHPT generated regression: $GLOBALS should have canonicalized keys
--DESCRIPTION--
original php-src path: Zend/tests/restrict_globals/key_canonicalization.phpt
original source hash: d8610cc88322b30b1cff73776683b9ffe608cc2f391545e9dac750788f27ae63
generated timestamp: 20260715T152632Z
generator version: phpt-generate-v1
reason: known target failure minimized against reference output
--FILE--
<?php
${1} = 42;
var_dump($GLOBALS[1]);
--EXPECT--
int(42)
