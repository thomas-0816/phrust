--TEST--
Generated zend.objects: clone creates distinct object identity
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-clone-v1
reason: Prompt 14.8 clone identity baseline
--FILE--
<?php
class CloneIdentityBox {
    public $value = 1;
}

$original = new CloneIdentityBox();
$copy = clone $original;
if ($original === $copy) {
    echo "same\n";
} else {
    echo "different\n";
}
?>
--EXPECT--
different
