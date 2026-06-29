--TEST--
Generated objects.clone: __clone runs on clone
--DESCRIPTION--
module: objects.clone
generated timestamp: 20260627T000000Z
generator version: phpt-objects-clone-v1
reason: __clone baseline
--FILE--
<?php
class CloneMagicBox {
    public $name = "original";

    public function __clone() {
        $this->name = "copy";
    }
}

$original = new CloneMagicBox();
$copy = clone $original;
echo $original->name, "|", $copy->name, "\n";
?>
--EXPECT--
original|copy
