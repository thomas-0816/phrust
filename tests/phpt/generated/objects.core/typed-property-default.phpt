--TEST--
Generated objects.core: typed property default value is initialized
--DESCRIPTION--
module: objects.core
generated timestamp: 20260628T000000Z
generator version: phpt-objects-core-v1
reason: Branch 1 object-core typed property default baseline
--FILE--
<?php
class Box {
    public int $value = 9;
}

$box = new Box();
echo $box->value, "\n";
$box->value = 10;
echo $box->value, "\n";
?>
--EXPECT--
9
10
