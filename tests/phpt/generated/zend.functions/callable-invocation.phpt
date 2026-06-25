--TEST--
Generated zend.functions: callable invocation through direct and helper paths
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260625T000000Z
generator version: phpt-zend-functions-v1
reason: direct callable, call_user_func, and call_user_func_array execution
--FILE--
<?php
function plus_one($value) {
    return $value + 1;
}

$callable = plus_one(...);
echo $callable(1), "|", call_user_func("plus_one", 2), "|", call_user_func_array("plus_one", [3]), "\n";
?>
--EXPECT--
2|3|4
