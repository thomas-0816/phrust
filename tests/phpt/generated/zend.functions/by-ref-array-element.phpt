--TEST--
Generated zend.functions: by-ref array element send mutates caller
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260627T000000Z
generator version: phpt-zend-functions-v1
reason: array element by-reference parameter sends use IR by_ref_dim metadata
--FILE--
<?php
function set_value(&$value): void {
    $value = "changed";
}

$items = ["key" => "start"];
set_value($items["key"]);
echo $items["key"], "\n";
?>
--EXPECT--
changed
