--TEST--
Generated zend.functions: variadic packing preserves positional and named tail
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260625T000000Z
generator version: phpt-zend-functions-v1
reason: user function variadic argument packing
--FILE--
<?php
function join_parts($first, ...$rest): void {
    echo $first, "|", $rest[0], "|", $rest["last"], "\n";
}

join_parts("a", "b", last: "c");
?>
--EXPECT--
a|b|c
