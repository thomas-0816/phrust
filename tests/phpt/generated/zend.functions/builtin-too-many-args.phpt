--TEST--
Generated zend.functions: builtin too many args is ArgumentCountError
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260625T000000Z
generator version: phpt-zend-functions-v1
reason: arginfo-backed builtin maximum arity handling
--FILE--
<?php
try {
    strlen("abc", "extra");
} catch (ArgumentCountError $e) {
    echo "too-many\n";
}
?>
--EXPECT--
too-many
