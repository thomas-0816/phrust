--TEST--
PHPT generated smoke: Creating instances dynamically
--DESCRIPTION--
original php-src path: Zend/tests/objects/objects_023.phpt
original source hash: e877c73955c26f3b0084c591e4eeb08d9e97b3d4a6f12ea344bab9b52866cc39
generated timestamp: 20260627T201250Z
generator version: phpt-generate-v1
reason: smallest reference-passing example
--FILE--
<?php

$arr = array(new stdClass, 'stdClass');

new $arr[0]();
new $arr[1]();

print "ok\n";

?>
--EXPECT--
ok
