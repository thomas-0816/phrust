--TEST--
Generated zend.functions: weak scalar coercion for internal functions
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260625T000000Z
generator version: phpt-zend-functions-v1
reason: weak-mode builtin scalar coercion uses generated arginfo
--FILE--
<?php
echo strlen(42), "|", strtoupper(42), "\n";
?>
--EXPECT--
2|42
