--TEST--
Generated objects.clone: clone creates distinct object identity
--DESCRIPTION--
module: objects.clone
generated timestamp: 20260627T000000Z
generator version: phpt-objects-clone-v1
reason: clone identity baseline
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
