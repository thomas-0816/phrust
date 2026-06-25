--TEST--
PHPT generated smoke: redefining constructor (__construct first)
--DESCRIPTION--
original php-src path: Zend/tests/objects/objects_011.phpt
original source hash: f7be0aa80fd9a7313c680ff7105b05775cd63c64c93a056ea5f46806400fec1c
generated timestamp: 20260625T154035Z
generator version: phpt-generate-v1
reason: smallest reference-passing example
--INI--
error_reporting=8191
--FILE--
<?php

class test {
    function __construct() {
    }
    function test() {
    }
}

echo "Done\n";
?>
--EXPECT--
Done
