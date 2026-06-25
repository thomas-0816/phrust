--TEST--
PHPT generated smoke: redefining constructor (__construct second)
--DESCRIPTION--
original php-src path: Zend/tests/objects/objects_010.phpt
original source hash: f4ce98afe047c4408e806c17cd6d1cea6416c36a93134928dfbc6d637724b87d
generated timestamp: 20260625T154035Z
generator version: phpt-generate-v1
reason: smallest reference-passing example
--FILE--
<?php

class test {
    function test() {
    }
    function __construct() {
    }
}

echo "Done\n";
?>
--EXPECT--
Done
