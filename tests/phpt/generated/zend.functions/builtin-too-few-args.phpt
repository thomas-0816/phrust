--TEST--
Generated zend.functions: builtin too few args is ArgumentCountError
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260625T000000Z
generator version: phpt-zend-functions-v1
reason: arginfo-backed builtin minimum arity handling
--FILE--
<?php
try {
    strlen();
} catch (ArgumentCountError $e) {
    echo "too-few\n";
}
?>
--EXPECT--
too-few
