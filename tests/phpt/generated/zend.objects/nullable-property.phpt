--TEST--
Generated zend.objects: nullable property accepts null and typed values
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-typed-properties-v1
reason: nullable property baseline
--FILE--
<?php
class Box {
    public ?int $value = null;
}

$box = new Box();
var_dump($box->value);
$box->value = 3;
echo $box->value, "\n";
$box->value = null;
var_dump($box->value);
?>
--EXPECT--
NULL
3
NULL
