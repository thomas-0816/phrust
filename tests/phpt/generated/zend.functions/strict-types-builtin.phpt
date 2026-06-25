--TEST--
Generated zend.functions: strict_types rejects internal scalar coercion
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260625T000000Z
generator version: phpt-zend-functions-v1
reason: strict-mode builtin scalar coercion uses generated arginfo
--FILE--
<?php
declare(strict_types=1);

try {
    ord(49);
} catch (TypeError $e) {
    echo "strict\n";
}
?>
--EXPECT--
strict
