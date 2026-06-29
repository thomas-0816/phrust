--TEST--
Generated objects.clone: clone-with typed property mismatch
--DESCRIPTION--
module: objects.clone
generated timestamp: 20260627T000000Z
generator version: phpt-objects-clone-v1
reason: clone-with typed property mismatch baseline
--FILE--
<?php
class CloneWithMismatchBox {
    public int $count = 1;
}

$original = new CloneWithMismatchBox();
try {
    $copy = clone($original, ["count" => []]);
} catch (TypeError $e) {
    echo get_class($e), "\n";
}
?>
--EXPECT--
TypeError
