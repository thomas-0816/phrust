--TEST--
Generated zend.objects: clone-with public property replacement
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-clone-v1
reason: Prompt 14.8 clone-with public property baseline
--FILE--
<?php
class CloneWithBox {
    public $name = "old";
    public $count = 1;
}

$original = new CloneWithBox();
$copy = clone($original, ["name" => "new", "count" => 2]);
echo $original->name, ":", $original->count, "|", $copy->name, ":", $copy->count, "\n";
?>
--EXPECT--
old:1|new:2
