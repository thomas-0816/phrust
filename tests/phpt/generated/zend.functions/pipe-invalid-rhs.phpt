--TEST--
Generated zend.functions: pipe RHS rejects non-callable values
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260627T000000Z
generator version: phpt-zend-functions-v1
reason: invalid pipe RHS values route to a catchable Error
--FILE--
<?php
try {
    2 |> 4;
} catch (Error $e) {
    echo "invalid\n";
}
?>
--EXPECT--
invalid
