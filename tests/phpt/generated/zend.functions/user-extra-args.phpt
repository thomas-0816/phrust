--TEST--
Generated zend.functions: user functions accept extra positional arguments
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260627T000000Z
generator version: phpt-zend-functions-v1
reason: a non-variadic user function accepts surplus positional arguments without ArgumentCountError; they are ignored for parameter binding but visible to func_get_args()/func_num_args() (Zend/tests/func_get_args_basic.phpt)
--FILE--
<?php
function one($a)
{
    return $a . '|' . implode(',', func_get_args());
}
function arity($x)
{
    return func_num_args();
}
echo one(1, 2, 3), "\n";
echo arity(10, 20), "\n";
?>
--EXPECT--
1|1,2,3
2
