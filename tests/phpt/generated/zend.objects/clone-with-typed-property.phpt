--TEST--
Generated zend.objects: clone-with typed public property replacement
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-clone-v1
reason: Prompt 14.8 clone-with typed property baseline
--FILE--
<?php
class CloneWithTypedBox {
    public int $count = 1;
}

$original = new CloneWithTypedBox();
$copy = clone($original, ["count" => 2]);
echo $original->count, "|", $copy->count, "\n";
?>
--EXPECT--
1|2
