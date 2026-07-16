--TEST--
PHPT generated smoke: Variable with integer name
--DESCRIPTION--
original php-src path: Zend/tests/variable_with_integer_name.phpt
original source hash: 48644f4034d7ddd2424675f87f9707651917777d875e6319a2288e188d983a36
generated timestamp: 20260715T152632Z
generator version: phpt-generate-v1
reason: smallest reference-passing example
--FILE--
<?php

${10} = 42;
var_dump(${10});

?>
--EXPECT--
int(42)
