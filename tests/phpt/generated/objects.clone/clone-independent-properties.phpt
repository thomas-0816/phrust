--TEST--
Generated objects.clone: cloned public properties are independent
--DESCRIPTION--
module: objects.clone
generated timestamp: 20260627T000000Z
generator version: phpt-objects-clone-v1
reason: independent clone property baseline
--FILE--
<?php
class ClonePropertyBox {
    public $value = 1;
}

$original = new ClonePropertyBox();
$copy = clone $original;
$copy->value = 2;
echo $original->value, "|", $copy->value, "\n";
?>
--EXPECT--
1|2
