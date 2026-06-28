--TEST--
Generated zend.functions: by-ref local send mutates caller
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260627T000000Z
generator version: phpt-zend-functions-v1
reason: local variable by-reference parameter sends use IR by_ref_local metadata
--FILE--
<?php
function set_value(&$value): void {
    $value = "changed";
}

$value = "start";
set_value($value);
echo $value, "\n";
?>
--EXPECT--
changed
