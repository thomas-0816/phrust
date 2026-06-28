--TEST--
Generated zend.functions: missing user args throw ArgumentCountError
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260627T000000Z
generator version: phpt-zend-functions-v1
reason: missing required user-function arguments route through call-site ArgumentCountError
--FILE--
<?php
function required_arg($value)
{
    return $value;
}
try {
    required_arg();
} catch (ArgumentCountError $e) {
    echo "caught\n";
    echo str_contains($e->getMessage(), "Too few arguments to function required_arg()") ? "message\n" : "bad\n";
}
?>
--EXPECT--
caught
message
