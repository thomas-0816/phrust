--TEST--
Generated zend.functions: strict_types rejects user scalar coercion
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260627T000000Z
generator version: phpt-zend-functions-v1
reason: strict-mode user function scalar parameter checks reject incompatible scalar values
--FILE--
<?php
declare(strict_types=1);

function prompt13_takes_int(int $value) {
    return $value;
}

try {
    prompt13_takes_int("42");
} catch (TypeError $e) {
    echo "strict-user\n";
}
?>
--EXPECT--
strict-user
