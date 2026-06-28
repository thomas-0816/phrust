--TEST--
Generated objects.core: public property read and write
--DESCRIPTION--
module: objects.core
generated timestamp: 20260627T000000Z
generator version: phpt-objects-basics-v1
reason: Prompt 14.3 public property read/write baseline
--FILE--
<?php
class Box {
    public $value = 1;
}

$box = new Box();
echo $box->value, "\n";
$box->value = 3;
echo $box->value, "\n";
?>
--EXPECT--
1
3
