--TEST--
PHPT generated smoke: Testing include
--DESCRIPTION--
original php-src path: tests/lang/015.phpt
original source hash: 47bbe4c65d694731e0e10979d1a61012db157f3bf26a4c090a2f8a658fea0962
generated timestamp: 20260715T154100Z
generator version: phpt-generate-v1
reason: smallest reference-passing example
--FILE--
<?php
include "015.inc";
?>
--EXPECT--

Warning: include(015.inc): Failed to open stream: No such file or directory in /data/src/ml/phrust/target/phpt-work/generate/unknown/final-1/test.php on line 2

Warning: include(): Failed opening '015.inc' for inclusion (include_path='.:') in /data/src/ml/phrust/target/phpt-work/generate/unknown/final-1/test.php on line 2
