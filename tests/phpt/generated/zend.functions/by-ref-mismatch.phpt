--TEST--
Generated zend.functions: by-ref parameter mismatch is rejected
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260625T000000Z
generator version: phpt-zend-functions-v1
reason: by-reference parameters require referenceable arguments
--FILE--
<?php
function set_value(&$value): void {
    $value = 2;
}

try {
    set_value(1);
} catch (Error $e) {
    echo "by-ref\n";
}
?>
--EXPECT--
by-ref
